use futures::StreamExt;
use itertools::enumerate;
use serenity::{
    async_trait,
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    http::Http,
    model::{channel::Message, prelude::ChannelId},
    Result as SerenityResult,
};

use crate::{Queue, Track};
use regex::Regex;
use serenity::model::id::GuildId;
use serenity::model::mention::Mentionable;
use serenity::prelude::TypeMap;
use songbird::{
    input::restartable::Restartable, Event, EventContext, EventHandler as VoiceEventHandler,
    TrackEvent,
};
use std::sync::Arc;
use tokio::sync::RwLock;

#[allow(dead_code)]
struct TrackEndNotifier {
    chan_id: ChannelId,
    http: Arc<Http>,
    data: Arc<RwLock<TypeMap>>,
    guild_id: GuildId,
}

#[async_trait]
impl VoiceEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(_track_list) = ctx {
            let queue_lock = {
                let data_read = self.data.read().await;
                data_read
                    .get::<Queue>()
                    .expect("Expected Queue in TypeMap.")
                    .clone()
            };
            {
                let mut queue = queue_lock.write().await;
                queue.remove(0);
            }
        }

        None
    }
}

trait StripBetween {
    fn strip_between(&self, first_delimiter: &str, second_delimiter: &str) -> Self;
}

impl StripBetween for String {
    fn strip_between(&self, first_delimiter: &str, second_delimiter: &str) -> Self {
        if self.contains(first_delimiter) && self.contains(second_delimiter) {
            let start_bytes = self.find(first_delimiter).unwrap();
            let end_bytes = self.find(second_delimiter).unwrap();
            let removal = &self[start_bytes..(end_bytes + 1)];
            self.replace(removal, "")
        } else {
            self.to_string()
        }
    }
}

#[command]
#[aliases(q, list, playlist)]
async fn queue(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let mut page_number = if args.is_empty() {
        1
    } else {
        match args.message().to_string().parse::<i32>() {
            Ok(n) => n,
            Err(_) => {
                msg.channel_id
                    .say(
                        &ctx.http,
                        format!(
                            "({}) is not a valid page number.",
                            args.message().to_string()
                        ),
                    )
                    .await?;
                1
            }
        }
    };

    let queue_lock = {
        let data_read = ctx.data.read().await;

        data_read
            .get::<Queue>()
            .expect("Expected queue in TypeMap.")
            .clone()
    };
    let playlist = queue_lock.read().await;

    if playlist.is_empty() {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Nothing is currently playing.")
                .await,
        );
        return Ok(());
    }
    let mut description = String::new();
    let mut pages: Vec<String> = Vec::new();

    let current_track = playlist.iter().next().unwrap();

    for (i, track) in enumerate(playlist.iter()) {
        let mut title = track.title.to_owned();
        title = title.strip_between("(", ")");
        title = title.strip_between("[", "]");

        description.push_str(&*format!("{}", i + 1));
        description.push(':');
        description.push(' ');
        description.push_str(&title);
        description.push(' ');
        description.push_str(&*format!("(Requested by: {})", &track.requester));
        description.push('\n');
        if description.matches('\n').count() > 10 {
            pages.push(description.clone());
            description = String::new();
        }
    }

    if pages.is_empty() || !description.is_empty() {
        pages.push(description);
    }

    if page_number <= 0 {
        page_number = 1
    }
    if page_number > pages.len() as i32 {
        page_number = pages.len() as i32
    }

    let page = match pages.get((page_number - 1) as usize) {
        Some(p) => p,
        None => pages.get(0).unwrap(),
    };

    check_msg(
        msg.channel_id
            .send_message(&ctx.http, |m| {
                m.embed(|e| {
                    e.author(|a| {
                        a.name(format!("Now playing: {}", &current_track.title))
                            .url(&current_track.url)
                            .icon_url("https://i.imgur.com/vVvNHcj.png")
                    })
                    .description(page)
                    .footer(|f| f.text(format!("Page: {}/{}", page_number, pages.len())))
                })
            })
            .await,
    );

    Ok(())
}

async fn play_youtube_video_url(
    ctx: &Context,
    msg: &Message,
    query: String,
    is_url: bool,
) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;
    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let source = match if is_url {
            Restartable::ytdl(query, true).await
        } else {
            Restartable::ytdl_search(query, true).await
        } {
            Ok(source) => source,
            Err(why) => {
                println!("Err starting source: {:?}", why);

                return Ok(());
            }
        };

        let track = handler.enqueue_source(source.into());

        let queue_lock = {
            let data_read = ctx.data.read().await;
            data_read
                .get::<Queue>()
                .expect("Expected Queue in TypeMap.")
                .clone()
        };
        {
            let mut queue = queue_lock.write().await;

            let metadata = &track.metadata();

            let title = match_else_none(&metadata.title);
            let thumbnail = match_else_none(&metadata.thumbnail);
            let artist = match_else_none(&metadata.artist);
            let channel = match_else_none(&metadata.channel);
            let date = match_else_none(&metadata.date);
            let url = match_else_none(&metadata.source_url);
            let duration = metadata.duration.to_owned();
            let starttime = metadata.start_time.to_owned();

            let track = Track {
                requester: msg.author.name.to_owned(),
                url,
                title,
                thumbnail,
                artist,
                channel,
                date,
                starttime,
                duration,
            };
            queue.push(track);
        }
    } else {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Not in a voice channel to play in")
                .await,
        );
    }
    Ok(())
}

async fn play_youtube_playlist(ctx: &Context, msg: &Message, url: String) -> CommandResult {
    let regex =
        Regex::new(r"(?:(?:PL|LL|EC|UU|FL|RD|UL|TL|PU|OLAK5uy_)[0-9A-Za-z-_]{10,}|RDMM)").unwrap();
    let playlist_id = regex
        .captures_iter(&url)
        .next()
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .unwrap()
        .as_str();

    let id = playlist_id.parse()?;
    let ytextract = ytextract::Client::new();

    let playlist = ytextract.playlist(id).await?;

    let videos = playlist.videos();
    futures::pin_mut!(videos);
    let mut to_be_enqueued: Vec<String> = Vec::new();

    while let Some(item) = videos.next().await {
        match item {
            Ok(video) => to_be_enqueued.push(format!("https://youtu.be/{}", video.id())),
            Err(err) => println!("{:#?},", err),
        }
    }

    for uri in to_be_enqueued {
        play_youtube_video_url(&ctx, &msg, uri, true).await?;
    }

    check_msg(
        msg.channel_id
            .say(&ctx.http, "Added playlist to queue.")
            .await,
    );
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn play(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let query = String::from(args.message());

    if query.contains("youtube") || query.contains("youtu.be") {
        if !query.contains("playlist?list") {
            return play_youtube_video_url(&ctx, &msg, query, true).await;
        } else {
            return play_youtube_playlist(&ctx, &msg, query).await;
        }
    } else {
        return play_youtube_video_url(&ctx, &msg, query, false).await;
    }
}
fn match_else_none(input: &Option<String>) -> String {
    match input {
        Some(n) => n.to_owned(),
        None => String::from("None"),
    }
}

#[command]
#[only_in(guilds)]
#[aliases(np, song)]
async fn playing(ctx: &Context, msg: &Message) -> CommandResult {
    let queue_lock = {
        let data_read = ctx.data.read().await;

        data_read
            .get::<Queue>()
            .expect("Expected queue in TypeMap.")
            .clone()
    };

    let playlist = queue_lock.read().await;

    if let Some(current_track) = playlist.iter().next() {
        let mut date = current_track.date.to_owned();
        date.insert(4, '\\');
        date.insert(4, '\\');
        date.insert(8, '\\');
        date.insert(8, '\\');

        check_msg(
            msg.channel_id
                .send_message(&ctx.http, |m| {
                    m.embed(|e| {
                        e.author(|a| {
                            a.name(format!("Now playing: {}", &current_track.title))
                                .url(&current_track.url)
                                .icon_url("https://i.imgur.com/vVvNHcj.png")
                        })
                        .field("Requested By:", &current_track.requester, true)
                        .thumbnail(&current_track.thumbnail)
                        .field("Uploaded By:", &current_track.channel, true)
                        .field("Upload Date:", &date, true)
                    })
                })
                .await,
        );
    } else {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Nothing is currently playing.")
                .await,
        );
    }
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let channel_id = guild
        .voice_states
        .get(&msg.author.id)
        .and_then(|voice_state| voice_state.channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            check_msg(msg.reply(ctx, "Not in a voice channel").await);

            return Ok(());
        }
    };

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let (handle_lock, success) = manager.join(guild_id, connect_to).await;

    if let Ok(_channel) = success {
        check_msg(
            msg.channel_id
                .say(&ctx.http, &format!("Joined {}", connect_to.mention()))
                .await,
        );

        let chan_id = msg.channel_id;

        let send_http = ctx.http.clone();

        let send_guild = msg.guild_id.unwrap().clone();

        let send_data = ctx.data.clone();

        let mut handle = handle_lock.lock().await;

        handle.add_global_event(
            Event::Track(TrackEvent::End),
            TrackEndNotifier {
                chan_id,
                http: send_http,
                data: send_data,
                guild_id: send_guild,
            },
        );
    } else {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Error joining the channel")
                .await,
        );
    }

    Ok(())
}

fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
