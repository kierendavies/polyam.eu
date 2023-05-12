mod cache;
mod messages;

use crate::commands::CommandContext;
use crate::config::GuildConfig;
use crate::error::bail;
use crate::error::Error;
use crate::error::Result;
use crate::onboarding::messages::get_intro_message;
use crate::FrameworkContext;
use anyhow::Context as _;
use poise::serenity_prelude::ActionRowComponent;
use poise::serenity_prelude::ChannelId;
use poise::serenity_prelude::Context;
use poise::serenity_prelude::CreateInteractionResponse;
use poise::serenity_prelude::GuildId;
use poise::serenity_prelude::InputTextStyle;
use poise::serenity_prelude::InteractionResponseType;
use poise::serenity_prelude::Member;
use poise::serenity_prelude::Message;
use poise::serenity_prelude::MessageComponentInteraction;
use poise::serenity_prelude::ModalSubmitInteraction;
use poise::serenity_prelude::RoleId;
use poise::serenity_prelude::User;
use poise::ApplicationCommandOrAutocompleteInteraction;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::info;
use tracing::warn;

use self::messages::delete_welcome_message;
use self::messages::edit_or_send_intro_message;
use self::messages::send_welcome_message;

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

#[tracing::instrument(skip_all)]
async fn submit_intro_quarantined(
    ctx: &Context,
    db: &PgPool,
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

    let guild_id = interaction
        .guild_id
        .context("Interaction has no guild_id")?;
    let mut member = interaction
        .member
        .clone()
        .context("Interaction has no member")?;
    let intro_fields = IntroFields::try_from(interaction)?;

    unquarantine(ctx, quarantine_role_id, &mut member).await?;
    edit_or_send_intro_message(
        ctx,
        db,
        guild_id,
        intros_channel_id,
        &interaction.user,
        &intro_fields,
    )
    .await?;
    interaction
        .delete_original_interaction_response(ctx)
        .await?;
    delete_welcome_message(ctx, db, guild_id, interaction.user.id).await?;

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn submit_intro_slash(
    ctx: &Context,
    db: &PgPool,
    intros_channel_id: ChannelId,
    interaction: &ModalSubmitInteraction,
) -> Result<()> {
    let guild_id = interaction
        .guild_id
        .context("Interaction has no guild_id")?;
    let intro_fields = IntroFields::try_from(interaction)?;
    let message = edit_or_send_intro_message(
        ctx,
        db,
        guild_id,
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
    let db = &framework.user_data.db;

    let mut member = member.clone();
    quarantine(ctx, config.quarantine_role, &mut member).await?;
    send_welcome_message(ctx, db, config.quarantine_channel, &member).await?;

    Ok(())
}

pub async fn guild_member_removal(
    ctx: &Context,
    framework: FrameworkContext<'_>,
    guild_id: &GuildId,
    user: &User,
) -> Result<()> {
    let db = &framework.user_data.db;

    delete_welcome_message(ctx, db, *guild_id, user.id).await?;

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
    let db = &framework.user_data.db;

    match interaction.data.custom_id.as_str() {
        ID_INTRO_QUARANTINE => {
            submit_intro_quarantined(
                ctx,
                db,
                config.quarantine_role,
                config.intros_channel,
                interaction,
            )
            .await?;
        }

        ID_INTRO_SLASH => {
            submit_intro_slash(ctx, db, config.intros_channel, interaction).await?;
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

    let intro_message = get_intro_message(
        ctx.serenity_context(),
        &ctx.data().db,
        guild_id,
        ctx.author().id,
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