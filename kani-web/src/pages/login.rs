use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

#[component]
pub fn Login() -> impl IntoView {
    let query = use_query_map();

    let error_msg = move || {
        query.with(|q| {
            match q.get("error").as_deref() {
                Some("invalid") => Some("Invalid username or password."),
                Some("server") => Some("Something went wrong — please try again."),
                _ => None,
            }
        })
    };

    view! {
        <div class="login-page">
            <div class="login-card">
                <div class="login-brand">
                    <span class="login-brand__icon">"<k>"</span>
                    <h1 class="login-brand__title">"Kani"</h1>
                </div>

                <p class="login-subtitle">"Sign in to your reader"</p>

                {move || error_msg().map(|e| view! {
                    <div class="login-error">
                        <span class="login-error__icon">"⚠"</span>
                        {e}
                    </div>
                })}

                <form
                    class="login-form"
                    method="post"
                    action="/rest/auth/login"
                >
                    <div class="login-field">
                        <label class="login-field__label" for="username">
                            "Username"
                        </label>
                        <input
                            class="login-field__input"
                            type="text"
                            id="username"
                            name="username"
                            autocomplete="username"
                            required
                            autofocus
                        />
                    </div>

                    <div class="login-field">
                        <label class="login-field__label" for="password">
                            "Password"
                        </label>
                        <input
                            class="login-field__input"
                            type="password"
                            id="password"
                            name="password"
                            autocomplete="current-password"
                            required
                        />
                    </div>

                    <button type="submit" class="login-btn">
                        "Sign In"
                    </button>
                </form>
            </div>
        </div>
    }
}
