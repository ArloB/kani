use sha2::{Digest, Sha384};
use sqlx::migrate::Migrator;
use sqlx::{Row, SqlitePool};

use crate::error::{Result, ServiceError};

pub(super) static MIGRATOR: Migrator = sqlx::migrate!("../migrations");

struct Transition {
    version: i64,
    legacy: &'static str,
    current: &'static str,
    semantic: &'static str,
}

const TRANSITIONS: &[Transition] = &[
    Transition {
        version: 20260604000005,
        legacy: "77f6ba83e42af3f24daffc90daa46251d6674158da7c800542b1f31cba5b76bf08d95637153c543803bd7ccc6d199643",
        current: "89b675ea8e95448a152389eb8b57f4a69ae3fd11a6b07de09a7e6e89aac8a9407588e837d5c17c633a6205914d12bbcd",
        semantic: "c7655d00b49fea907a4afefb7fb40d6a719da7c34dbf4e8845f4244f7308e9529472cf36147c67799f3c6218659b74d0",
    },
    Transition {
        version: 20260621000001,
        legacy: "1efe727a8a9d35042afbd0c2dad6a4845fd86c8b41cffb162ed42d8fd1bae4d6d95c227deae699abc1a5c629f32baf66",
        current: "99db346ca732312fc07ef957445e45350755b4e230b0a35ac76a17ea3147a92af83e7cd227bce22c8e57f7478fbecbfc",
        semantic: "feceac6ff6598209563943b570318c63509e17915f38d8215a6eb6167e90cdf545158300f47660f9da6f8b1684c6afd6",
    },
    Transition {
        version: 20260721000003,
        legacy: "9328879d43527e4d8106db0e653166d8c08e3ee9a6bda48813573a6fbdbc80f63abf300d76dc1990e5f55326fbda4879",
        current: "8e39b4a5b80d09e60f0a4030c1a27cbec3f49c89cdb06c454cc08a62104e8ad7ff9b66d5fdb54e585e120092ee9aec4f",
        semantic: "97c0deb1762658bb71b1a7d1fc429adbe4f550c257ad49b3d6fe55a1e092681e3eb8e5746c3eadb3a867694a7e006221",
    },
    Transition {
        version: 20260721000004,
        legacy: "007594de2938a19fda180f68781200ea486aa5c660244676e4e2325f91c88d9b58d85ec15f5c2c000cac18a9c6c0570a",
        current: "882d8d173282530ddeeba60420f5a5a9efbdb4cb34fe7f479ba35d56969030dc0e320dfe07cf49ccdd8e36e364161c30",
        semantic: "bed1d916923a0d6fe7c95c468eacf7329501dad960d5e60ffad5fce1ee0dd6f45c9de64974d26a790361ff2a91662ea3",
    },
    Transition {
        version: 20260722000004,
        legacy: "4cf8bfbe850fb8b569c614ef6a8d75b79dcf6f7837aed492629009c6c6da404c2ecd7490bcda78cfe678ccd084260a3f",
        current: "fc8c8c434cac393889906879e9353966d513d33c5b5b759af2b9ed3712d0b4d417994adcd2bae0be7ae4b31897f70730",
        semantic: "6ec74ee332f3fe35e0169afbd6959699819deb29ce33cd22f7e804d975513133596c035c6cd22e92bef7eeda6aa78c4e",
    },
    Transition {
        version: 20260722000005,
        legacy: "dc74e347120e10063b334cfe620bbb7c2eaeda7f77ece8f58a05dad61bd22bc459696682e8b35d0adfee8f6635ca6191",
        current: "db1f79932db805fc4bc888e10b1498a15ae544f3ba01a83bd8f5c57899f4dda8f0bdabc8d05fe016c4da23b3f1cba590",
        semantic: "3301d88a8334915e5a528ac198cc334641a683d2517e2bdbf97b75256c9d418da35761c2011940b193e7be896ea91088",
    },
    Transition {
        version: 20260722000006,
        legacy: "ddf7e4e1345becd369dcccd704b13eb014d6c4ceea5d99818efae7d883b2972973d6bc96639fd8adb2d7cd09ca3bcf62",
        current: "ef04ebfa05f68a8cab526b35ef1de3c25537ab6b2d78846e66572dc20f4503213d661ebdc7897736b2bbe6d6e550fd73",
        semantic: "74b53e81a844262e58967f0fd3e548f94275b21edd0cbcee211626367cd4db93044ca9f386fc9d6e68514b0eb0d3deb0",
    },
    Transition {
        version: 20260722000007,
        legacy: "ea6e6b246f3cda82fdcd115af74f1a00d5e50df6542baca17271bed556b6ef2f679d604f56dc3e4a485fc6e8b03d4143",
        current: "87114df37eeca513e9197ac653ddefeb97047ec4bc53a2d5782c1377d652f50c8708f4beab4c4d2d22b2bdd2c6105c36",
        semantic: "cf0e3c31b1e4ff8cdb3c63f81bde679e77dabce94c73fe86adff0a39f09d170b9a1a51cf0fcd4ccb5363e91bddf559cd",
    },
    Transition {
        version: 20260722000008,
        legacy: "aa2b85f6e5f1543f8b6e13ae19a0c3b0699c74785c5e6ad274424afa10ddbb44e38d9c771d2d09a75865fdd287c30502",
        current: "efdd0541dccd889259f7f26a0a603138a63b4818227624397b1e1760204c21e93d92c5d47896814cf75573eb5453f053",
        semantic: "db5427b005b7b24bd627d85e35a693af5e0c7595f0411ef0b506a59888bd4ceec0709ddc18a812646cbe94d35146cbb5",
    },
    Transition {
        version: 20260722000009,
        legacy: "884f6f250c9551130db58d571250cd171910e7f6d25037b89f92b3a9c592ba141165141b79cda69fa4ab08072e6b2c02",
        current: "6c4da97acf2f7eddc69e5b1ebe5e4fb376ff2ce1eb7868f187b8b0ce99f622d3a76e73f8b58a562ee5e3459a5ead9586",
        semantic: "68bd81a783e5521a6703e225dd54c068670ba584737d0e17ad36db2cbc348bf3ece1181e3abb4d1f3dd9001eb4f03c45",
    },
    Transition {
        version: 20260722000010,
        legacy: "77c32805a9db2e32be106fe2d03c92420e5d8bb284d4c1c7a581dbc5e041430d4a1600b7f87c905915460f4adb547d22",
        current: "3e9baae6b2a925b72388e7dc338b6b47d72a4a1aef7bd54d05f1a5908aabdbb27a99917b3988269e4567ab0520bde5c9",
        semantic: "7a12c8142763b287e7770994c60e3d322fb2171ce47c2ac243a9adfa56ffe584e5fe39be871ab6f9262d040b223ae53e",
    },
    Transition {
        version: 20260722000011,
        legacy: "a71710fe0b5d2850ffcd4493d57b271a6f12b5124d1e95d3f9651abc2a57d1645ad727ae6c9e75e21e68673da255739a",
        current: "53bdd3a1d902912174b73cd4e3996ec95a07b9e94aa1c74b237480f6c5af251024073fac5e221d0fff9eeef5dcdc82ce",
        semantic: "dd3922484a4cdb2003bf9a58740b3bfd3bccbc3271424de54cc6612159ac822e22d8ef58cf14e2b6f9fe5ac5498544dc",
    },
    Transition {
        version: 20260722000012,
        legacy: "28d6d1307734e4a0fa08c5d51424d91d21298c0b2ecea7d6fa98d29d45f8c89d3fced5aa870bf8e5b082764d645f6b28",
        current: "c9936ffbe8ad15e0abf9d15b0ffbc7a4b5a68c594196ece7d6cb59cd9d795eda3272f2f0b16f49cc698a90b9adf09b9d",
        semantic: "0e619d3e387bdf1accd4b89f5b0e4a0b5621ea8f47162183ef805bdc17ab43de05bc7f57e0dc4c89ca4980229ee1895c",
    },
    Transition {
        version: 20260727000001,
        legacy: "ff81ad604cd355637df002fedf5533f154e2fb4036980abdd57f48f736fc4ec56800254ad5e38317a6642606cea161fd",
        current: "fcf74ab8a721cd8034f3e359616bceed601b15feec235e5845b0bfc6f2b6ace8402afa555b4c8b5c12bf5c5846286a2a",
        semantic: "82b1bd82dd3fff02ae389d6e57b161c2353e4f55a116c8bb8cfecae7e901031e656e9cbb0ef191ee8bac811ee89885c4",
    },
    Transition {
        version: 20260728000001,
        legacy: "6a36017806ef5af4b3decb3589bb4c06a136c1eb25bfef098b66cdf563f5cc03125b435cab465568c7c5212fa81225cb",
        current: "a84267985ccad8a088bc0e579a65b16674a38c273d12517df92c66fa91c507625c5e20e082b2292ac715133358ef0d56",
        semantic: "7aeec10a946ace5dc6c539d21367bb0d37982fc4ba5f5568cf377010a771ef37b9ce8d1bf49f59becba8f2705425538a",
    },
    Transition {
        version: 20260729000001,
        legacy: "7463e4747f6ceff5a8256aaacce4f3b8e6124818e3fc2b147c7ff2ed3b30aea034d4421651418e8f2c01e3b6835e8ea0",
        current: "e92916a2eed7732feb2e28a1e916f559fd35f91a0c7f44285a18a2002d107566404c9213d48e25a0f65756e018b47eee",
        semantic: "1445164e14e4ac93080d2e7f5d886b6b680d2f6a7d9ca5880ab6a60f9d0885f32bb895e1a4deef8965069d03bcfe6820",
    },
    Transition {
        version: 20260729000002,
        legacy: "3280d90adafcdb3f082c9b881e3bc91c7e906fc465f5acccc21508ee2cc6e3146f3edc28f40353967ac9dd45342fd6fd",
        current: "22f08fd4d9c3501218f0e21d8c923c0af6958539065455f827447f6ad1bafdf559a63c0cf00f1f858d47ed2958fff3a9",
        semantic: "3fecef24ecda3081b6b9f748709004c0889b98aff91f943fcd778cb2e7a68320ed2873643f44852dc4ad6d75c942a5f8",
    },
    Transition {
        version: 20260731000001,
        legacy: "50431adf76ca30931b95009ce3b475f8a123d913721b9bcfec49e8b7bea1ea9aabb2ccef278d3937af364fc75a44cae6",
        current: "20dad3b392cd1a17ebc685cb7ebd2cfe42fef15144507d567293c470fabd925cc3a4eb5e5ada107530bf8cc4ae55cf81",
        semantic: "4ed112bf54a8041d45fdeac9b57c632062378929a7e042e219e2c72d213bb100426e56292e7e3c293dc97035c767ec94",
    },
    Transition {
        version: 20260731000002,
        legacy: "9e6d747ae653dcfa1395ee64a273c61298fd65a231a352d2159a9868cb1411f4f06d55d15f252b04fe36d6adc06f0577",
        current: "c415cd08c89211821d4c7d96f9af7454b5d54b593ff424e3200b7119cf4c1d7ed3ed97e222c9b181f9aeac1647208ac3",
        semantic: "38588b03a2a041d2065d86b7f20e85d12c12c6b1d0b43f8b9d5f1855a9a1c21b2f7596a3036079715df4c26a38051943",
    },
];

pub(super) async fn run(pool: &SqlitePool) -> Result<()> {
    reconcile(pool).await?;
    MIGRATOR.run(pool).await?;
    Ok(())
}

async fn reconcile(pool: &SqlitePool) -> Result<u64> {
    validate_transitions()?;
    let exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;
    if exists == 0 {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let rows = sqlx::query("SELECT version, checksum, success FROM _sqlx_migrations")
        .fetch_all(&mut *tx)
        .await?;
    let mut changed = 0;
    for transition in TRANSITIONS {
        let Some(row) = rows
            .iter()
            .find(|row| row.get::<i64, _>("version") == transition.version)
        else {
            continue;
        };
        if !row.get::<bool, _>("success") {
            continue;
        }
        let legacy = decode(transition.legacy)?;
        if row.get::<Vec<u8>, _>("checksum") != legacy {
            continue;
        }
        let current = decode(transition.current)?;
        let result = sqlx::query(
            "UPDATE _sqlx_migrations SET checksum = ? WHERE version = ? AND checksum = ? AND success = TRUE",
        )
        .bind(&current)
        .bind(transition.version)
        .bind(&legacy)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(ServiceError::Internal(format!(
                "migration checksum transition raced for version {}",
                transition.version
            )));
        }
        changed += 1;
    }
    tx.commit().await?;
    if changed > 0 {
        tracing::info!(changed, "Reconciled comment-only migration checksums");
    }
    Ok(changed)
}

fn validate_transitions() -> Result<()> {
    for transition in TRANSITIONS {
        let migration = MIGRATOR
            .iter()
            .find(|migration| migration.version == transition.version)
            .ok_or_else(|| {
                ServiceError::Internal(format!(
                    "migration checksum transition references missing version {}",
                    transition.version
                ))
            })?;
        if migration.checksum.as_ref() != decode(transition.current)? {
            return Err(ServiceError::Internal(format!(
                "migration {} changed after its checksum transition was recorded",
                transition.version
            )));
        }
        let semantic = hex::encode(Sha384::digest(normalize_sql(&migration.sql).as_bytes()));
        if semantic != transition.semantic {
            return Err(ServiceError::Internal(format!(
                "migration {} contains an executable change",
                transition.version
            )));
        }
    }
    Ok(())
}

fn decode(value: &str) -> Result<Vec<u8>> {
    hex::decode(value).map_err(|error| ServiceError::Internal(error.to_string()))
}

fn normalize_sql(source: &str) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Normal,
        LineComment,
        BlockComment,
        Single,
        Double,
        Bracket,
    }

    let chars: Vec<char> = source.chars().collect();
    let mut output = String::new();
    let mut state = State::Normal;
    let mut whitespace = false;
    let mut index = 0;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        match state {
            State::Normal if current == '-' && next == Some('-') => {
                state = State::LineComment;
                whitespace = true;
                index += 2;
            }
            State::Normal if current == '/' && next == Some('*') => {
                state = State::BlockComment;
                whitespace = true;
                index += 2;
            }
            State::Normal if current.is_whitespace() => {
                whitespace = true;
                index += 1;
            }
            State::Normal => {
                if whitespace && !output.is_empty() && !output.ends_with(' ') {
                    output.push(' ');
                }
                whitespace = false;
                output.push(current);
                state = match current {
                    '\'' => State::Single,
                    '"' => State::Double,
                    '[' => State::Bracket,
                    _ => State::Normal,
                };
                index += 1;
            }
            State::LineComment => {
                if current == '\n' {
                    state = State::Normal;
                }
                index += 1;
            }
            State::BlockComment if current == '*' && next == Some('/') => {
                state = State::Normal;
                whitespace = true;
                index += 2;
            }
            State::BlockComment => index += 1,
            quoted => {
                let close = match quoted {
                    State::Single => '\'',
                    State::Double => '"',
                    State::Bracket => ']',
                    _ => unreachable!(),
                };
                output.push(current);
                if current == close && next == Some(close) {
                    output.push(close);
                    index += 2;
                } else {
                    if current == close {
                        state = State::Normal;
                    }
                    index += 1;
                }
            }
        }
    }
    output.trim().to_owned()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    async fn pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    #[test]
    fn transition_manifest_matches_embedded_migrations() {
        validate_transitions().unwrap();
    }

    #[test]
    fn normalization_ignores_comments_but_preserves_quoted_markers() {
        let left = "SELECT '--x', \"/*y*/\", [--z] FROM t; -- old";
        let right = "SELECT '--x', \"/*y*/\", [--z] FROM t; /* new */";
        assert_eq!(normalize_sql(left), normalize_sql(right));
        assert_ne!(normalize_sql("SELECT 'a'"), normalize_sql("SELECT 'b'"));
    }

    #[tokio::test]
    async fn legacy_checksums_are_reconciled_before_validation() {
        let pool = pool().await;
        MIGRATOR.run(&pool).await.unwrap();
        for transition in TRANSITIONS {
            sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
                .bind(decode(transition.legacy).unwrap())
                .bind(transition.version)
                .execute(&pool)
                .await
                .unwrap();
        }

        run(&pool).await.unwrap();

        for transition in TRANSITIONS {
            let checksum: Vec<u8> =
                sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = ?")
                    .bind(transition.version)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(checksum, decode(transition.current).unwrap());
        }
    }

    #[tokio::test]
    async fn unknown_checksum_still_fails_closed() {
        let pool = pool().await;
        MIGRATOR.run(&pool).await.unwrap();
        let version = TRANSITIONS[0].version;
        sqlx::query("UPDATE _sqlx_migrations SET checksum = zeroblob(48) WHERE version = ?")
            .bind(version)
            .execute(&pool)
            .await
            .unwrap();

        let error = run(&pool).await.unwrap_err();
        assert!(matches!(
            error,
            ServiceError::Migration(sqlx::migrate::MigrateError::VersionMismatch(found))
                if found == version
        ));
    }

    #[tokio::test]
    async fn fresh_database_needs_no_reconciliation() {
        let pool = pool().await;
        assert_eq!(reconcile(&pool).await.unwrap(), 0);
        run(&pool).await.unwrap();
    }
}
