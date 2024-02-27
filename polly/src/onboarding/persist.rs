macro_rules! message_cache_impl {
    (set: $query:literal) => {
        #[allow(clippy::cast_possible_wrap)]
        #[::tracing::instrument(skip(db))]
        pub async fn set<'db, DB: ::sqlx::PgExecutor<'db>>(
            db: DB,
            guild_id: ::serenity::all::GuildId,
            user_id: ::serenity::all::UserId,
            channel_id: ::serenity::all::ChannelId,
            message_id: ::serenity::all::MessageId,
        ) -> $crate::error::Result<()> {
            ::sqlx::query!(
                $query,
                guild_id.get() as i64,
                user_id.get() as i64,
                channel_id.get() as i64,
                message_id.get() as i64,
            )
            .execute(db)
            .await?;

            Ok(())
        }
    };

    (get: $query:literal) => {
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        #[::tracing::instrument(skip(db))]
        pub async fn get<'db, DB: ::sqlx::PgExecutor<'db>>(
            db: DB,
            guild_id: ::serenity::all::GuildId,
            user_id: ::serenity::all::UserId,
        ) -> $crate::error::Result<
            Option<(
                ::serenity::all::ChannelId,
                ::serenity::all::MessageId,
            )>,
        > {
            let row = ::sqlx::query!($query, guild_id.get() as i64, user_id.get() as i64)
                .map(|record| {
                    (
                        ::serenity::all::ChannelId::new(record.channel_id as u64),
                        ::serenity::all::MessageId::new(record.message_id as u64),
                    )
                })
                .fetch_optional(db)
                .await?;

            Ok(row)
        }
    };

    (get_all: $query:literal) => {
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        #[::tracing::instrument(skip(db))]
        pub async fn get_all<'db, DB: ::sqlx::PgExecutor<'db>>(
            db: DB,
            guild_id: ::serenity::all::GuildId,
        ) -> $crate::error::Result<
            Vec<(
                ::serenity::all::UserId,
                ::serenity::all::ChannelId,
                ::serenity::all::MessageId,
            )>,
        > {
            let rows = ::sqlx::query!($query, guild_id.get() as i64)
            .map(|record| {
                (
                    ::serenity::all::UserId::new(record.user_id as u64),
                    ::serenity::all::ChannelId::new(record.channel_id as u64),
                    ::serenity::all::MessageId::new(record.message_id as u64),
                )
            })
            .fetch_all(db)
            .await?;

            Ok(rows)
        }
    };

    (delete: $query:literal) => {
        #[allow(clippy::cast_possible_wrap)]
        #[::tracing::instrument(skip(db))]
        pub async fn delete<'db, DB: ::sqlx::PgExecutor<'db>>(
            db: DB,
            guild_id: ::serenity::all::GuildId,
            user_id: ::serenity::all::UserId,
        ) -> $crate::error::Result<()> {
            let query_result = ::sqlx::query!($query, guild_id.get() as i64, user_id.get() as i64)
                .execute(db)
                .await?;

            if query_result.rows_affected() == 0 {
                $crate::error::bail!("No rows deleted")
            }

            Ok(())
        }
    };

    ($($op:ident: $query:literal),* $(,)?) => {
        $(message_cache_impl! { $op: $query })*
    };
}

pub mod welcome_message {
    message_cache_impl! {
        set: "insert into onboarding_welcome_messages (guild_id, user_id, channel_id, message_id) values ($1, $2, $3, $4)",
        get: "select channel_id, message_id from onboarding_welcome_messages where guild_id = $1 and user_id = $2",
        delete: "delete from onboarding_welcome_messages where guild_id = $1 and user_id = $2",
    }
}

pub mod intro_message {
    message_cache_impl! {
        set: "insert into onboarding_intro_messages (guild_id, user_id, channel_id, message_id) values ($1, $2, $3, $4)",
        get: "select channel_id, message_id from onboarding_intro_messages where guild_id = $1 and user_id = $2",
        get_all: "select user_id, channel_id, message_id from onboarding_intro_messages where guild_id = $1",
        delete: "delete from onboarding_intro_messages where guild_id = $1 and user_id = $2",
    }
}
