//! Parser for the declarative extraction expression language.

mod parseexpr;

use crate::dsl::parseexpr::ParseExpr;
use chumsky::prelude::*;
use kani_shared::ast::Op;

pub use self::parseexpr::SpannedParseExpr;

type ParserError<'a> = extra::Err<Rich<'a, char>>;

/// Builds a parser that preserves source spans for conversion and validation diagnostics.
pub fn parser<'a>() -> impl Parser<'a, &'a str, SpannedParseExpr, ParserError<'a>> {
    let hws = || {
        any::<&str, ParserError>()
            .filter(|c: &char| *c == ' ' || *c == '\t')
            .repeated()
            .ignored()
    };

    let ws = || {
        let block_comment = just("/*")
            .ignore_then(
                none_of(['*'])
                    .ignored()
                    .or(just('*').then_ignore(none_of(['/'])).ignored())
                    .repeated(),
            )
            .then_ignore(just("*/"))
            .ignored();
        let any_ws = any::<&str, ParserError>()
            .filter(|c: &char| c.is_whitespace())
            .repeated()
            .at_least(1)
            .ignored();
        choice((any_ws, block_comment)).repeated().ignored()
    };

    let ident = text::ident().map(|s: &str| s.to_string()).padded_by(ws());

    let variable = just('$')
        .ignore_then(text::ident())
        .map(|s: &str| format!("${s}"))
        .padded_by(ws());

    let string_literal = just('"')
        .ignore_then(none_of('"').repeated().collect::<String>())
        .then_ignore(just('"'))
        .padded_by(ws());

    let number = just('-')
        .or_not()
        .then(text::digits(10))
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .map(|s: &str| {
            ParseExpr::Number(
                s.parse::<f64>()
                    .expect("grammar restricts this slice to valid float syntax"),
            )
        })
        .padded_by(ws());

    let val_bool = choice((
        text::keyword("true").to(ParseExpr::Bool(true)),
        text::keyword("false").to(ParseExpr::Bool(false)),
    ))
    .padded_by(ws());

    let val_null = text::keyword("null").to(ParseExpr::Null).padded_by(ws());

    let val_index = text::keyword("index")
        .ignore_then(just('('))
        .ignore_then(just(')'))
        .to(ParseExpr::Index)
        .padded_by(ws());

    let val_self = text::keyword("self").to(ParseExpr::SelfRef).padded_by(ws());

    let map_entry = string_literal
        .then_ignore(just(':').padded_by(ws()))
        .then(string_literal);

    let map_literal = map_entry
        .separated_by(just(',').padded_by(ws()))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just('{'), just('}'))
        .padded_by(ws())
        .map(ParseExpr::MapLiteral);

    let built_in_str = |name| {
        text::keyword(name)
            .ignore_then(string_literal.delimited_by(just('('), just(')')))
            .padded_by(ws())
    };

    let dom = built_in_str("dom").map(ParseExpr::Dom);
    let json = built_in_str("json").map(ParseExpr::Json);
    let pref = built_in_str("pref").map(ParseExpr::Pref);

    let terminator = choice((just(';').ignored(), just('\n').ignored())).padded_by(hws());

    let expr = recursive(|expr| {
        let comma_list = expr
            .clone()
            .separated_by(just(','))
            .allow_trailing()
            .collect::<Vec<_>>();

        let array = comma_list
            .clone()
            .delimited_by(just('['), just(']'))
            .map(ParseExpr::List);

        let merge = text::keyword("merge")
            .ignore_then(
                comma_list
                    .clone()
                    .delimited_by(just('['), just(']'))
                    .delimited_by(just('('), just(')')),
            )
            .map(ParseExpr::Merge);

        let format = text::keyword("format")
            .ignore_then(
                string_literal
                    .then(
                        just(',')
                            .padded_by(ws())
                            .ignore_then(comma_list.clone())
                            .or_not()
                            .map(|opt| opt.unwrap_or_default()),
                    )
                    .delimited_by(just('('), just(')')),
            )
            .map(|(template, args)| ParseExpr::Format { template, args });

        let let_expr = text::keyword("let")
            .ignore_then(variable)
            .then_ignore(just('=').padded_by(ws()))
            .then(expr.clone())
            .then_ignore(terminator)
            .then(expr.clone())
            .map(|((name, value), body)| ParseExpr::Let {
                name,
                value: Box::new(value),
                body: Box::new(body),
            });

        let if_then_else = text::keyword("if")
            .ignore_then(expr.clone())
            .then_ignore(text::keyword("then").padded_by(ws()))
            .then(expr.clone())
            .then_ignore(text::keyword("else").padded_by(ws()))
            .then(expr.clone())
            .map(|((cond, then_b), else_b)| ParseExpr::If {
                condition: Box::new(cond),
                then: Box::new(then_b),
                else_: Box::new(else_b),
            });

        let arg_choice = choice((map_literal, expr.clone()));

        let arg_list = arg_choice
            .separated_by(just(','))
            .allow_trailing()
            .collect::<Vec<_>>();

        let atom = choice((
            let_expr,
            val_self,
            val_null,
            val_bool,
            val_index,
            dom,
            json,
            pref,
            number,
            string_literal.map(ParseExpr::Literal),
            variable.map(ParseExpr::Var),
            merge,
            format,
            if_then_else,
            array,
            expr.clone().delimited_by(just('('), just(')')),
        ))
        .boxed();

        let fn_ident = text::ident().map(|s: &str| s.to_string()).padded_by(ws());

        let user_fn_call = text::keyword("user")
            .padded_by(ws())
            .ignore_then(just('.'))
            .ignore_then(fn_ident)
            .then(arg_list.clone().delimited_by(just('('), just(')')))
            .map_with(|(fn_name, args), extra| (format!("__user::{fn_name}"), args, extra.span()));

        let method_call = ident.then(arg_list.delimited_by(just('('), just(')')));

        let chain = atom
            .foldl(
                hws()
                    .ignore_then(just('.'))
                    .ignore_then(choice((
                        user_fn_call,
                        method_call.map_with(|(name, args), extra| (name, args, extra.span())),
                    )))
                    .repeated(),
                |target, (name, args, span)| ParseExpr::MethodCall {
                    target: Box::new(target),
                    name,
                    args,
                    span,
                },
            )
            .boxed();

        let mul_op = choice((just('*').to(Op::Mul), just('/').to(Op::Div)));
        let mul_expr = chain
            .clone()
            .foldl(mul_op.then(chain).repeated(), |lhs, (op, rhs)| {
                ParseExpr::BinaryOperation {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                }
            });

        let add_op = choice((just('+').to(Op::Add), just('-').to(Op::Sub)));
        let add_expr =
            mul_expr
                .clone()
                .foldl(add_op.then(mul_expr).repeated(), |lhs, (op, rhs)| {
                    ParseExpr::BinaryOperation {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    }
                });

        let cmp_op = choice((
            just("==").to(Op::Eq),
            just("!=").to(Op::Ne),
            just("<=").to(Op::Le),
            just(">=").to(Op::Ge),
            just('<').to(Op::Lt),
            just('>').to(Op::Gt),
        ));
        let cmp_expr =
            add_expr
                .clone()
                .foldl(cmp_op.then(add_expr).repeated(), |lhs, (op, rhs)| {
                    ParseExpr::BinaryOperation {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    }
                });

        let and_expr = cmp_expr.clone().foldl(
            just("&&").to(Op::And).then(cmp_expr).repeated(),
            |lhs, (op, rhs)| ParseExpr::BinaryOperation {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        );

        and_expr.clone().foldl(
            just("||").to(Op::Or).then(and_expr).repeated(),
            |lhs, (op, rhs)| ParseExpr::BinaryOperation {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        )
    });

    expr.map_with(|e, extra| SpannedParseExpr(e, extra.span()))
        .then_ignore(end())
}
