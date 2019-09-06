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
use chrono::{Duration, Utc};
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
    let stand_up_config = get_config(&api_key).expect("Config error");
    let web_cli = api::requests::default_client().unwrap();
    let listener_thread = thread::spawn(move || {
        let max_retries = 10;
        let mut handler = MyHandler { sender };
        let mut retries = 0;
        let mut last_error = Utc::now() - Duration::days(36500);
        loop {
            let r = RtmClient::login_and_run(&api_key, &mut handler);
            match r {
                Ok(_) => {}
                Err(err) => {
                    let time_since_last_error = Utc::now() - last_error;
                    if time_since_last_error > Duration::minutes(1) {
                        retries = 1;
                    } else {
                        retries += 1;
                    }
                    last_error = Utc::now();
                    if retries > max_retries {
                        panic!("Listener thread error: {}", err);
                    } else {
                        println!("Listener thread error, retry {}: {}", retries, err);
                    }
                }
            }
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
            Err(err) => panic!("Web thread error: {}", err),
        }
    });
    listener_thread.join().unwrap();
    stand_up_bot_thread.join().unwrap();
    web_thread.join().unwrap();
}
