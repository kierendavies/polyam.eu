use super::option_value;
use super::respond_with_content;
use super::Command;
use super::CommandError;
use once_cell::sync::Lazy;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use rand::Rng;
use serenity::async_trait;
use serenity::builder::CreateApplicationCommand;
use serenity::model::prelude::command::CommandOptionType;
use serenity::model::prelude::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::interaction::application_command::CommandDataOptionValue;
use serenity::prelude::Context;

const BUBBLES: [(&str, u32); 3] = [("🔵", 240), ("💥", 10), ("🐱", 1)];

const OPT_SIZE: &str = "size";

fn sample_bubble<R: Rng + ?Sized>(rng: &mut R) -> &str {
    static DISTRIBUTION: Lazy<WeightedIndex<u32>> =
        Lazy::new(|| WeightedIndex::new(BUBBLES.map(|b| b.1)).unwrap());

    BUBBLES[DISTRIBUTION.sample(rng)].0
}

pub struct Bubblewrap;

#[async_trait]
impl Command for Bubblewrap {
    const NAME: &'static str = "bubblewrap";

    fn create_application_command<'a>(
        &self,
        command: &'a mut CreateApplicationCommand,
    ) -> &'a mut CreateApplicationCommand {
        command
            .name(Self::NAME)
            .description("Sends you some bubble wrap to pop. Might contain bombs.")
            .create_option(|opt| {
                opt.name(OPT_SIZE)
                    .description("Size of the square of bubbles")
                    .kind(CommandOptionType::Integer)
                    .min_int_value(1)
                    .max_int_value(19)
            })
    }

    async fn handle_command_interaction(
        &self,
        ctx: Context,
        interaction: &ApplicationCommandInteraction,
    ) -> Result<(), CommandError> {
        let size = option_value!(
            interaction.data.options,
            OPT_SIZE,
            CommandDataOptionValue::Integer,
            5
        )?;

        let mut text = String::new();
        {
            let mut rng = rand::thread_rng();
            for _ in 0..size {
                for _ in 0..size {
                    text.push_str(&format!("||{}||", sample_bubble(&mut rng)));
                }
                text.push('\n');
            }
        }

        respond_with_content(ctx, interaction, text).await?;
        Ok(())
    }
}
