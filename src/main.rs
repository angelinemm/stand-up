mod bot;
mod config;
mod handler;
mod utils;

use crate::bot::Bot;
use crate::config::get_config;
use crate::handler::MyHandler;
use slack::{api, Message as SlackMessage, RtmClient};
use std::env;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
#[macro_use]
#[cfg(test)]
extern crate assert_matches;

fn main() {
    let api_key: String = env::var("API_KEY").expect("Need API key");
    let (sender, receiver): (Sender<SlackMessage>, Receiver<SlackMessage>) = mpsc::channel();
    let ws_cli = RtmClient::login(&api_key).expect("Can't login websocket client");
    let stand_up_config = get_config(&ws_cli).expect("Config error");
    let web_cli = api::requests::default_client().unwrap();
    let listener_thread = thread::spawn(move || {
        let mut handler = MyHandler { sender };
        let r = ws_cli.run(&mut handler);
        match r {
            Ok(_) => {}
            Err(err) => panic!("Error: {}", err),
        }
    });
    let stand_up_bot_thread = thread::spawn(move || {
        let mut bot = Bot::new(web_cli, receiver, stand_up_config).unwrap();
        bot.stand_up_machine();
    });
    listener_thread.join().unwrap();
    stand_up_bot_thread.join().unwrap();
}
