use crate::utils::{TeamMember, TimeOfDay};
use slack::{RtmClient, User as SlackUser};
use std::env;

pub struct Config {
    pub api_key: String,
    pub channel_id: String,
    pub team_members: Vec<TeamMember>,
    pub stand_up_time: TimeOfDay,
    pub number_of_questions: u8,
    pub questions: Vec<String>,
}

pub fn get_config(client: &RtmClient) -> Result<Config, ()> {
    let api_key: String = env::var("API_KEY").map_err(|_| ())?;
    let channel: String = env::var("CHANNEL").map_err(|_| ())?;
    let stand_up_time =
        TimeOfDay::from_str(&env::var("STAND_UP_TIME").map_err(|_| ())?).map_err(|_| ())?;
    let number_of_questions: u8 = env::var("NUMBER_OF_QUESTIONS")
        .map_err(|_| ())?
        .parse()
        .unwrap();
    let questions: Vec<String> = (1..=number_of_questions)
        .map(|i| env::var(&format!("Q{}", i)).unwrap())
        .collect();
    let team_members: Vec<String> = env::var("TEAM_MEMBERS")
        .unwrap()
        .split(',')
        .map(ToString::to_string)
        .collect();
    let channel_id = client
        .start_response()
        .channels
        .as_ref()
        .and_then(|channels| {
            channels.iter().find(|chan| match chan.name {
                None => false,
                Some(ref name) => name == &channel,
            })
        })
        .and_then(|chan| chan.id.clone())
        .expect("Could not find channel for stand-up :( ");
    let users: Vec<&SlackUser> = client
        .start_response()
        .users
        .as_ref()
        .expect("No users found")
        .iter()
        .filter(|user| match user.name {
            None => false,
            Some(ref name) => team_members.contains(name),
        })
        .collect();
    let team_members: Vec<TeamMember> = client
        .start_response()
        .ims
        .as_ref()
        .expect("No direct messages found")
        .iter()
        .filter_map(|dm| match users.iter().find(|user| user.id == dm.user) {
            None => None,
            Some(ref user) => Some(TeamMember {
                name: user
                    .name
                    .clone()
                    .unwrap_or_else(|| "Unknown name".to_string()),
                id: user.id.clone().expect("User without an id"),
                dm_id: dm.id.clone().expect("DM without an id"),
            }),
        })
        .collect();
    Ok(Config {
        api_key,
        channel_id,
        team_members,
        stand_up_time,
        number_of_questions,
        questions,
    })
}
