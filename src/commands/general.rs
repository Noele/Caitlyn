use itertools::enumerate;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::cmp::Ordering;

#[command]
async fn ping(context: &Context, msg: &Message) -> CommandResult {
    msg.channel_id.say(&context.http, "Pong!").await?;

    Ok(())
}

fn tuplesort(a: &(&UserId, i64), b: &(&UserId, i64)) -> Ordering {
    if a.1 < b.1 {
        return Ordering::Less;
    } else if a.1 > b.1 {
        return Ordering::Greater;
    }
    return Ordering::Equal;
}

#[command]
async fn userinfo(ctx: &Context, msg: &Message) -> CommandResult {
    let user = match msg.mentions.first() {
        Some(user) => user,
        None => &msg.author,
    };

    match msg.guild(&ctx.cache) {
        Some(guild) => {
            //Member position

            let mut membervec: Vec<(&UserId, i64)> = Vec::new();
            for (user_id, member) in &guild.members {
                match member.joined_at {
                    None => {}
                    Some(timestamp) => {
                        membervec.push((user_id, timestamp.timestamp()));
                    }
                }
            }
            membervec.sort_by(tuplesort);
            let mut member_position = 0;
            for (e, item) in enumerate(membervec) {
                if item.0 .0 == user.id.0 {
                    member_position = e + 1;
                }
            }

            match guild.members.get(&user.id) {
                Some(member) => {
                    //Join date

                    let join_date = match &member.joined_at {
                        None => Timestamp::now(),
                        Some(date) => date.to_owned(),
                    };
                    let formatted_date = join_date.format("%A, %d. %B %Y").to_string();

                    //Role Names
                    let mut rolenames = String::new();
                    for role in &member.roles {
                        match guild.roles.get(role) {
                            None => {}
                            Some(role) => {
                                rolenames.push_str("<@&");
                                rolenames.push_str(&role.id.0.to_string());
                                rolenames.push('>');
                                rolenames.push('\n');
                            }
                        }
                    }

                    msg.channel_id
                        .send_message(&ctx.http, |m| {
                            m.embed(|e| {
                                e.color(0xFFC0CB)
                                    .description(format!(
                                        "{} chilling in {} mode",
                                        &user.name,
                                        match guild.presences.get(&user.id) {
                                            Some(presence) => {
                                                presence.status.name()
                                            }
                                            None => {
                                                "offline"
                                            }
                                        }
                                    ))
                                    .timestamp(msg.timestamp)
                                    .field(
                                        "Nick",
                                        match &member.nick {
                                            None => "None",
                                            Some(nick) => nick,
                                        },
                                        true,
                                    )
                                    .field("Member No.", &member_position.to_string(), true)
                                    .field(
                                        "Account Created",
                                        &member
                                            .user
                                            .created_at()
                                            .format("%A, %d. %B %Y")
                                            .to_string(),
                                        true,
                                    )
                                    .field("Join Date", &formatted_date, true)
                                    .field(
                                        "Roles",
                                        if rolenames.is_empty() {
                                            "None"
                                        } else {
                                            &rolenames
                                        },
                                        true,
                                    )
                                    .field("User ID", &user.id.as_u64().to_string(), true)
                                    .thumbnail(&user.face())
                                    .author(|a| {
                                        a.name(&user.name).icon_url(match &guild.icon_url() {
                                            None => user.face(),
                                            Some(icon) => icon.to_string(),
                                        })
                                    })
                            })
                        })
                        .await?;
                }
                None => {
                    msg.channel_id
                        .say(&ctx.http, "Failed to get member.")
                        .await?;
                }
            }
        }
        None => {
            msg.channel_id.say(&ctx.http, "This command can only be used in a guild! Reason:||Online status's and roles are now linked to guilds.||").await?;
        }
    }
    Ok(())
}
