use super::respond_with_content;
use super::Command;
use anyhow::Result;
use once_cell::sync::Lazy;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use serenity::async_trait;
use serenity::builder::CreateApplicationCommand;
use serenity::model::prelude::interaction::application_command::ApplicationCommandInteraction;
use serenity::prelude::Context;

const BUBBLES: [(&str, u32); 3] = [("🔵", 240), ("💥", 10), ("🐱", 1)];

static DISTRIBUTION: Lazy<WeightedIndex<u32>> = Lazy::new(|| {
    println!("init");
    WeightedIndex::new(BUBBLES.map(|b| b.1)).unwrap()
});

const SIZE: u32 = 5;

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
    }

    async fn handle_command_interaction(
        &self,
        ctx: Context,
        interaction: &ApplicationCommandInteraction,
    ) -> Result<()> {
        let mut text = String::new();
        {
            let mut rng = rand::thread_rng();
            for _ in 0..SIZE {
                for _ in 0..SIZE {
                    let bubble = BUBBLES[DISTRIBUTION.sample(&mut rng)].0;
                    text.push_str(&format!("||{bubble}||"));
                }
                text.push('\n');
            }
        }

        respond_with_content(ctx, interaction, text).await?;
        Ok(())
    }
}
