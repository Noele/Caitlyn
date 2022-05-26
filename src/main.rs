mod commands;

use crate::commands::general::*;
use crate::commands::music::*;

#[macro_use]
extern crate tracing;

use std::env;

use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    framework::{
        standard::{
            macros::{group, hook},
            CommandResult,
        },
        StandardFramework,
    },
    model::{channel::Message, gateway::Ready, id::GuildId},
};

use serenity::prelude::*;
use songbird::SerenityInit;
use std::sync::Arc;
use std::time::Duration;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn cache_ready(&self, _: Context, _guilds: Vec<GuildId>) {
        info!("cache is ready!");
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }
}

#[hook]
async fn after(_ctx: &Context, _msg: &Message, command_name: &str, command_result: CommandResult) {
    match command_result {
        Err(why) => info!(
            "Command '{}' returned error {:?} => {}",
            command_name, why, why
        ),
        _ => (),
    }
}

#[group]
#[only_in(guilds)]
#[commands(ping, userinfo)]
struct General;

#[group]
#[only_in(guilds)]
#[commands(join, play, playing, queue, stop, skip)]
struct Music;

#[allow(dead_code)]
struct Track {
    url: String,
    requester: String,
    title: String,
    thumbnail: String,
    artist: String,
    channel: String,
    date: String,
    duration: Option<Duration>,
    starttime: Option<Duration>,
}

impl Track {}

struct Queue;

impl TypeMapKey for Queue {
    type Value = Arc<RwLock<Vec<Track>>>;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().expect("Failed to load .env file");

    tracing_subscriber::fmt::init();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("~"))
        .after(after)
        .group(&MUSIC_GROUP)
        .group(&GENERAL_GROUP);

    let intents = GatewayIntents::all();

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Error, client failed to build");

    {
        let mut data = client.data.write().await;
        data.insert::<Queue>(Arc::new(RwLock::new(Vec::new())));
    }
    let _ = client
        .start()
        .await
        .map_err(|why| warn!("Client ended: {:?}", why));

    Ok(())
}
