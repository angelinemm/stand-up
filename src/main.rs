mod bot;
mod config;
mod handler;
mod rest;
mod utils;

use crate::bot::Bot;
use crate::config::get_config;
use crate::handler::MyHandler;
use crate::rest::ping;
use actix_web::{web, App, HttpServer};
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
    let web_thread = thread::spawn(move || {
        let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
        println!("Web listening on 0.0.0.0:{}", port);
        let r = HttpServer::new(|| {
            App::new()
                .service(web::resource("/ping").to(ping))
                .service(web::resource("/").to(ping))
        })
        .disable_signals()
        .bind(format!("0.0.0.0:{}", port))
        .expect("Could not spawn a web server!")
        .run();
        match r {
            Ok(_) => {}
            Err(err) => panic!("Error: {}", err),
        }
    });
    listener_thread.join().unwrap();
    stand_up_bot_thread.join().unwrap();
    web_thread.join().unwrap();
}
