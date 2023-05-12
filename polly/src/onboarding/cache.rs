macro_rules! message_cache_impl {
    (set: $query:literal) => {
        #[allow(clippy::cast_possible_wrap)]
        #[::tracing::instrument(skip(db))]
        pub async fn set<'db, DB: ::sqlx::PgExecutor<'db>>(
            db: DB,
            guild_id: ::poise::serenity_prelude::GuildId,
            user_id: ::poise::serenity_prelude::UserId,
            channel_id: ::poise::serenity_prelude::ChannelId,
            message_id: ::poise::serenity_prelude::MessageId,
        ) -> $crate::error::Result<()> {
            ::sqlx::query!(
                $query,
                guild_id.0 as i64,
                user_id.0 as i64,
                channel_id.0 as i64,
                message_id.0 as i64,
            )
            .execute(db)
            .await?;

            Ok(())
        }
    };

    (get: $query:literal) => {
        #[allow(clippy::cast_possible_wrap)]
        #[allow(clippy::cast_sign_loss)]
        #[::tracing::instrument(skip(db))]
        pub async fn get<'db, DB: ::sqlx::PgExecutor<'db>>(
            db: DB,
            guild_id: ::poise::serenity_prelude::GuildId,
            user_id: ::poise::serenity_prelude::UserId,
        ) -> $crate::error::Result<
            Option<(
                ::poise::serenity_prelude::ChannelId,
                ::poise::serenity_prelude::MessageId,
            )>,
        > {
            let message_id = ::sqlx::query!($query, guild_id.0 as i64, user_id.0 as i64)
                .fetch_optional(db)
                .await?
                .map(|record| {
                    (
                        ::poise::serenity_prelude::ChannelId(record.channel_id as u64),
                        ::poise::serenity_prelude::MessageId(record.message_id as u64),
                    )
                });

            Ok(message_id)
        }
    };

    (delete: $query:literal) => {
        #[allow(clippy::cast_possible_wrap)]
        #[::tracing::instrument(skip(db))]
        pub async fn delete<'db, DB: ::sqlx::PgExecutor<'db>>(
            db: DB,
            guild_id: ::poise::serenity_prelude::GuildId,
            user_id: ::poise::serenity_prelude::UserId,
        ) -> $crate::error::Result<()> {
            let query_result = ::sqlx::query!($query, guild_id.0 as i64, user_id.0 as i64)
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
        delete: "delete from onboarding_intro_messages where guild_id = $1 and user_id = $2",
    }
}
