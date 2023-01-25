use std::collections::HashMap;
use std::future::Future;

use crate::commands::CommandContext;
use crate::config::GuildConfig;
use crate::error::bail;
use crate::error::Error;
use crate::error::Result;
use crate::FrameworkContext;
use anyhow::Context as _;
use poise::futures_util::future;
use poise::futures_util::StreamExt;
use poise::futures_util::TryStreamExt;
use poise::serenity_prelude::ActionRowComponent;
use poise::serenity_prelude::ChannelId;
use poise::serenity_prelude::Context;
use poise::serenity_prelude::CreateEmbed;
use poise::serenity_prelude::CreateInteractionResponse;
use poise::serenity_prelude::CreateMessage;
use poise::serenity_prelude::GuildId;
use poise::serenity_prelude::InputTextStyle;
use poise::serenity_prelude::InteractionResponseType;
use poise::serenity_prelude::Member;
use poise::serenity_prelude::Message;
use poise::serenity_prelude::MessageComponentInteraction;
use poise::serenity_prelude::ModalSubmitInteraction;
use poise::serenity_prelude::Permissions;
use poise::serenity_prelude::RoleId;
use poise::serenity_prelude::Timestamp;
use poise::serenity_prelude::User;
use poise::serenity_prelude::UserId;
use poise::ApplicationCommandOrAutocompleteInteraction;
use tracing::info;
use tracing::warn;

pub const ID_PREFIX: &str = "onboarding_";

const ID_INTRO_QUARANTINE: &str = "onboarding_intro_quarantine";
const ID_INTRO_SLASH: &str = "onboarding_intro_slash";
const ID_ABOUT_ME: &str = "about_me";
const ID_POLYAMORY_EXPERIENCE: &str = "polyamory_experience";

const LABEL_INTRODUCE_YOURSELF: &str = "Introduce yourself";
const LABEL_ABOUT_ME: &str = "About me";
const LABEL_POLYAMORY_EXPERIENCE: &str = "Polyamory experience";

struct IntroFields<'a> {
    about_me: &'a str,
    polyamory_experience: &'a str,
}

impl<'a> TryFrom<&'a ModalSubmitInteraction> for IntroFields<'a> {
    type Error = Error;

    fn try_from(interaction: &'a ModalSubmitInteraction) -> Result<Self> {
        let fields: HashMap<_, _> = interaction
            .data
            .components
            .iter()
            .flat_map(|row| row.components.iter())
            .filter_map(|component| match component {
                ActionRowComponent::InputText(input_text) => {
                    Some((input_text.custom_id.as_str(), input_text.value.as_str()))
                }
                _ => None,
            })
            .collect();

        let field = |key| {
            fields
                .get(key)
                .with_context(|| format!("Missing field: {key:?}"))
        };

        Ok(IntroFields {
            about_me: field(ID_ABOUT_ME)?,
            polyamory_experience: field(ID_POLYAMORY_EXPERIENCE)?,
        })
    }
}

impl<'a> TryFrom<&'a Message> for IntroFields<'a> {
    type Error = Error;

    fn try_from(message: &'a Message) -> Result<Self> {
        let embed = message.embeds.first().context("Message has no embeds")?;

        let fields: HashMap<_, _> = embed
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.value.as_str()))
            .collect();

        let field = |key| {
            fields
                .get(key)
                .with_context(|| format!("Missing field: {key:?}"))
        };

        Ok(IntroFields {
            about_me: field(LABEL_ABOUT_ME)?,
            polyamory_experience: field(LABEL_POLYAMORY_EXPERIENCE)?,
        })
    }
}

#[tracing::instrument(skip(framework))]
fn guild_config(framework: FrameworkContext<'_>, guild_id: GuildId) -> Result<&GuildConfig> {
    Ok(framework
        .user_data
        .config
        .guilds
        .get(&guild_id)
        .context("No config for guild")?)
}

#[tracing::instrument(skip(ctx, f))]
async fn find_message<Fut, F>(
    ctx: &Context,
    channel_id: ChannelId,
    after: Option<Timestamp>,
    f: F,
) -> Result<Option<Message>>
where
    Fut: Future<Output = bool> + Send,
    F: FnMut(&Message) -> Fut + Send,
{
    let message = channel_id
        .messages_iter(ctx)
        .try_take_while(|message| {
            future::ok(match after {
                Some(after) => message.timestamp >= after,
                None => true,
            })
        })
        .try_filter(f)
        .boxed()
        .try_next()
        .await?;

    Ok(message)
}

#[tracing::instrument(skip_all)]
async fn quarantine(ctx: &Context, role_id: RoleId, member: &mut Member) -> Result<()> {
    member.add_role(ctx, role_id).await?;
    info!(
        %member.guild_id,
        %member.user.id,
        member.user.tag = member.user.tag(),
        "Quarantined member"
    );
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn unquarantine(ctx: &Context, role_id: RoleId, member: &mut Member) -> Result<()> {
    member.remove_role(ctx, role_id).await?;
    info!(
        %member.guild_id,
        %member.user.id,
        member.user.tag = member.user.tag(),
        "Unquarantined member"
    );
    Ok(())
}

fn create_welcome_message<'a, 'b>(
    guild_name: &str,
    member: &Member,
    message: &'b mut CreateMessage<'a>,
) -> &'b mut CreateMessage<'a> {
    let content = format!(
        "Welcome to {guild_name}, {member}! Please introduce yourself before you can start chatting.\n\
        \n\
        **Rules**\n\
        1. **DM = BAN**. This server is not for dating or hookups.\n\
        2. You must be at least 18 years old.\n\
        3. Always follow the Code of Conduct, available at https://polyam.eu/coc.html.\n\
        4. Speak English in the common channels."
    );

    message.content(content).components(|components| {
        components.create_action_row(|row| {
            row.create_button(|button| {
                button
                    .custom_id(ID_INTRO_QUARANTINE)
                    .label(LABEL_INTRODUCE_YOURSELF)
                    .emoji('👋')
            })
        })
    })
}

#[tracing::instrument(skip_all)]
async fn send_welcome_message(
    ctx: &Context,
    channel_id: ChannelId,
    member: &Member,
) -> Result<Message> {
    let channel = channel_id
        .to_channel(ctx)
        .await?
        .guild()
        .context("Not a guild channel")?;

    assert!(channel.guild_id == member.guild_id);

    let guild = member.guild_id.to_partial_guild(ctx).await?;

    let perms = guild.user_permissions_in(&channel, member)?;
    if !perms.contains(Permissions::VIEW_CHANNEL) {
        bail!(
            "Missing VIEW_CHANNEL permission: guild.id={}, guild.name={:?}, channel.id={}, channel.name={:?}, member.user.id={}, member.user.tag={:?}",
            guild.id,
            guild.name,
            channel.id,
            channel.name,
            member.user.id,
            member.user.tag(),
        );
    }

    let message = channel
        .id
        .send_message(ctx, |message| {
            create_welcome_message(&guild.name, member, message)
        })
        .await?;

    Ok(message)
}

#[tracing::instrument(skip_all)]
async fn delete_welcome_message(
    ctx: &Context,
    bot_id: UserId,
    member: &Member,
    message: &Message,
) -> Result<()> {
    fn has_intro_button(message: &Message) -> bool {
        message
            .components
            .iter()
            .flat_map(|row| row.components.iter())
            .any(|c| match c {
                ActionRowComponent::Button(button) => match &button.custom_id {
                    Some(id) => id == ID_INTRO_QUARANTINE,
                    None => false,
                },
                _ => false,
            })
    }

    // If the user clicked their own button, we can directly delete the message.
    // Otherwise we need to find the right message to delete.
    if message.mentions_user(&member.user) {
        message.delete(ctx).await?;
    } else if let Some(message) = find_message(
        ctx,
        message.channel_id,
        member.joined_at,
        |message: &Message| {
            future::ready(
                message.author.id == bot_id
                    && has_intro_button(message)
                    && message.mentions_user(&member.user),
            )
        },
    )
    .await?
    {
        message.delete(ctx).await?;
    } else {
        bail!(
            "No intro message found: member.guild_id={}, member.user.id={}, member.user.tag={:?}",
            member.guild_id,
            member.user.id,
            member.user.tag(),
        );
    }
    Ok(())
}

fn create_intro_modal<'a, 'b>(
    id: &str,
    intro_fields: Option<&IntroFields>,
    response: &'b mut CreateInteractionResponse<'a>,
) -> &'b mut CreateInteractionResponse<'a> {
    response
        .kind(InteractionResponseType::Modal)
        .interaction_response_data(|data| {
            data.custom_id(id)
                .title(LABEL_INTRODUCE_YOURSELF)
                .components(|components| {
                    components
                        .create_action_row(|row| {
                            row.create_input_text(|text| {
                                text.custom_id(ID_ABOUT_ME)
                                    .label(LABEL_ABOUT_ME)
                                    .style(InputTextStyle::Paragraph)
                                    .placeholder("I like long walks on the beach... 🏖")
                                    .required(true)
                                    .min_length(50);
                                if let Some(intro_fields) = intro_fields {
                                    text.value(intro_fields.about_me);
                                }
                                text
                            })
                        })
                        .create_action_row(|row| {
                            row.create_input_text(|text| {
                                text.custom_id(ID_POLYAMORY_EXPERIENCE)
                                    .label(LABEL_POLYAMORY_EXPERIENCE)
                                    .style(InputTextStyle::Paragraph)
                                    .placeholder("It's okay if you have none 💕")
                                    .required(true);
                                if let Some(intro_fields) = intro_fields {
                                    text.value(intro_fields.polyamory_experience);
                                }
                                text
                            })
                        })
                })
        })
}

fn create_intro_embed<'a>(
    user: &User,
    intro_fields: &IntroFields,
    embed: &'a mut CreateEmbed,
) -> &'a mut CreateEmbed {
    embed
        .description(format!("{user}"))
        .field(LABEL_ABOUT_ME, intro_fields.about_me, false)
        .field(
            LABEL_POLYAMORY_EXPERIENCE,
            intro_fields.polyamory_experience,
            false,
        );
    if let Some(avatar_url) = user.static_avatar_url() {
        embed.thumbnail(avatar_url);
    }
    embed
}

#[tracing::instrument(skip_all)]
async fn send_intro_message(
    ctx: &Context,
    channel_id: ChannelId,
    user: &User,
    intro_fields: &IntroFields<'_>,
) -> Result<Message> {
    let message = channel_id
        .send_message(ctx, |message| {
            message
                .content(format!("Introduction: {user}"))
                .embed(|embed| create_intro_embed(user, intro_fields, embed))
        })
        .await?;
    Ok(message)
}

#[tracing::instrument(skip_all)]
async fn find_intro_message(
    ctx: &Context,
    bot_id: UserId,
    channel_id: ChannelId,
    user: &User,
) -> Result<Option<Message>> {
    find_message(ctx, channel_id, None, |message| {
        future::ready(
            message.author.id == bot_id
                && !message.embeds.is_empty()
                && message.mentions_user(user),
        )
    })
    .await
}

#[tracing::instrument(skip_all)]
async fn edit_intro_message(
    ctx: &Context,
    message: &mut Message,
    user: &User,
    intro_fields: &IntroFields<'_>,
) -> Result<()> {
    message
        .edit(ctx, |message| {
            message.embed(|embed| create_intro_embed(user, intro_fields, embed))
        })
        .await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn edit_or_send_intro_message(
    ctx: &Context,
    bot_id: UserId,
    channel_id: ChannelId,
    user: &User,
    intro_fields: &IntroFields<'_>,
) -> Result<Message> {
    if let Some(mut message) = find_intro_message(ctx, bot_id, channel_id, user).await? {
        edit_intro_message(ctx, &mut message, user, intro_fields).await?;
        Ok(message)
    } else {
        send_intro_message(ctx, channel_id, user, intro_fields).await
    }
}

#[tracing::instrument(skip_all)]
async fn submit_intro_quarantined(
    ctx: &Context,
    bot_id: UserId,
    quarantine_role_id: RoleId,
    intros_channel_id: ChannelId,
    interaction: &ModalSubmitInteraction,
) -> Result<()> {
    let ack_content = "Thanks for submitting your introduction. In the next few seconds, you'll get access to the rest of the server.";
    interaction
        .create_interaction_response(ctx, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|data| data.content(ack_content).ephemeral(true))
        })
        .await?;

    let mut member = interaction
        .member
        .clone()
        .context("Interaction has no member")?;
    let welcome_message = interaction
        .message
        .as_ref()
        .context("Interaction has no message")?;
    let intro_fields = IntroFields::try_from(interaction)?;

    unquarantine(ctx, quarantine_role_id, &mut member).await?;
    edit_or_send_intro_message(
        ctx,
        bot_id,
        intros_channel_id,
        &interaction.user,
        &intro_fields,
    )
    .await?;
    interaction
        .delete_original_interaction_response(ctx)
        .await?;
    delete_welcome_message(ctx, bot_id, &member, welcome_message).await?;

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn submit_intro_slash(
    ctx: &Context,
    bot_id: UserId,
    intros_channel_id: ChannelId,
    interaction: &ModalSubmitInteraction,
) -> Result<()> {
    let intro_fields = IntroFields::try_from(interaction)?;
    let message = edit_or_send_intro_message(
        ctx,
        bot_id,
        intros_channel_id,
        &interaction.user,
        &intro_fields,
    )
    .await?;
    let message_url = message.link_ensured(ctx).await;

    interaction
        .create_interaction_response(ctx, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|data| {
                    data.content(format!("Introduction updated {message_url}"))
                    // Putting the URL in a button causes an error. Discord bug?
                    // Invalid Form Body (data.components.0.components.0.custom_id: A custom id is required)
                    // .components(|components| {
                    //     components.create_action_row(|row| {
                    //         row.create_button(|button| {
                    //             button.label("See your introduction").url(message_url)
                    //         })
                    //     })
                    // })
                })
        })
        .await?;

    Ok(())
}

#[tracing::instrument(
    fields(
        %member.guild_id,
        %member.user.id,
        member.user.tag = member.user.tag(),
    ),
    skip_all,
)]
pub async fn guild_member_addition(
    ctx: &Context,
    framework: FrameworkContext<'_>,
    member: &Member,
) -> Result<()> {
    let config = guild_config(framework, member.guild_id)?;

    let mut member = member.clone();
    quarantine(ctx, config.quarantine_role, &mut member).await?;
    send_welcome_message(ctx, config.quarantine_channel, &member).await?;

    Ok(())
}

#[tracing::instrument(
    fields(
        %interaction.id,
        interaction.guild_id = %interaction.guild_id.unwrap_or_default(),
        %interaction.user.id,
        interaction.user.tag = interaction.user.tag(),
        interaction.data.custom_id,
    ),
    skip_all,
)]
pub async fn message_component_interaction(
    ctx: &Context,
    _framework: FrameworkContext<'_>,
    interaction: &MessageComponentInteraction,
) -> Result<()> {
    match interaction.data.custom_id.as_str() {
        ID_INTRO_QUARANTINE => {
            interaction
                .create_interaction_response(ctx, |response| {
                    create_intro_modal(ID_INTRO_QUARANTINE, None, response)
                })
                .await?;
        }

        _ => bail!("Unhandled custom_id: {:?}", interaction.data.custom_id),
    }

    Ok(())
}

#[tracing::instrument(
    fields(
        %interaction.id,
        interaction.guild_id = %interaction.guild_id.unwrap_or_default(),
        %interaction.user.id,
        interaction.user.tag = interaction.user.tag(),
        interaction.data.custom_id,
    ),
    skip_all,
)]
pub async fn modal_submit_interaction(
    ctx: &Context,
    framework: FrameworkContext<'_>,
    interaction: &ModalSubmitInteraction,
) -> Result<()> {
    let guild_id = interaction
        .guild_id
        .context("Interaction has no guild_id")?;
    let config = guild_config(framework, guild_id)?;

    match interaction.data.custom_id.as_str() {
        ID_INTRO_QUARANTINE => {
            submit_intro_quarantined(
                ctx,
                framework.bot_id,
                config.quarantine_role,
                config.intros_channel,
                interaction,
            )
            .await?;
        }

        ID_INTRO_SLASH => {
            submit_intro_slash(ctx, framework.bot_id, config.intros_channel, interaction).await?;
        }

        _ => bail!("Unhandled custom_id: {:?}", interaction.data.custom_id),
    }

    Ok(())
}

/// Edit your introduction
#[poise::command(slash_command)]
#[tracing::instrument(
    fields(
        ctx.id = ctx.id(),
        ctx.guild_id = %ctx.guild_id().unwrap_or_default(),
        ctx.author.id = %ctx.author().id,
        ctx.author.tag = ?ctx.author().tag(),
    ),
    skip(ctx),
)]
pub async fn intro(ctx: CommandContext<'_>) -> Result<()> {
    let poise::Context::Application(app_ctx) = ctx else {
        bail!("Expected ApplicationContext");
    };
    let ApplicationCommandOrAutocompleteInteraction::ApplicationCommand(interaction) = app_ctx.interaction else {
        bail!("Expected ApplicationCommandInteraction");
    };

    let guild_id = ctx.guild_id().context("Context has no guild_id")?;
    let config = guild_config(ctx.framework(), guild_id)?;

    let intro_message = find_intro_message(
        ctx.serenity_context(),
        ctx.framework().bot_id,
        config.intros_channel,
        ctx.author(),
    )
    .await?;

    let intro_fields = intro_message
        .as_ref()
        .map(IntroFields::try_from)
        .transpose()
        .unwrap_or_else(|error| {
            warn!(?error, "Error getting intro fields");
            None
        });

    interaction
        .create_interaction_response(ctx, |response| {
            create_intro_modal(ID_INTRO_SLASH, intro_fields.as_ref(), response)
        })
        .await?;

    Ok(())
}
