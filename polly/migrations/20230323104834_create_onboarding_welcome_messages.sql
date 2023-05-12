create table onboarding_welcome_messages (
    guild_id bigint not null,
    user_id bigint not null,
    channel_id bigint not null,
    message_id bigint not null,
    primary key (guild_id, user_id)
)
