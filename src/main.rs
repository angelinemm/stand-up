mod bot;
mod handler;
mod utils;

use crate::bot::Bot;
use crate::handler::MyHandler;
use crate::utils::get_stand_up_config;
use config;
use slack::{api, Message as SlackMessage, RtmClient};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

fn main() {
    let mut config = config::Config::default();
    config.merge(config::File::with_name("Settings")).unwrap();
    let api_key: String = config.get_str("api_key").unwrap();
    let (sender, receiver): (Sender<SlackMessage>, Receiver<SlackMessage>) = mpsc::channel();
    let ws_cli = RtmClient::login(&api_key).expect("Can't login websocket client");
    let stand_up_config = get_stand_up_config(&ws_cli, &config).expect("Config error");
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
