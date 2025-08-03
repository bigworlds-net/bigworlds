use std::{io, str::FromStr, time::Duration};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{Terminal, backend::Backend};

use bigworlds::{
    Query,
    client::{self, AsyncClient, Client},
    net::CompositeAddress,
    query::{self, Trigger},
    rpc::msg::Message,
    string,
};

use crate::{ui, world::World};

/// Default poll duration.
pub const DEF_DUR: Duration = Duration::from_millis(400);
/// Pause duration.
pub const PAUSE: Duration = Duration::from_secs(60 * 60 * 24);

pub struct Queue {
    pub sender: tokio::sync::mpsc::Sender<Message>,
    pub receiver: tokio::sync::mpsc::Receiver<World>,
}

pub struct App {
    pub queue: Queue,

    pub world: World,
    pub poll_t: Duration,
}

impl App {
    pub async fn new(server_addr: CompositeAddress) -> Result<Self> {
        let (viewer_client_tx, mut viewer_client_rx) = tokio::sync::mpsc::channel::<Message>(10);
        let (client_viewer_tx, client_viewer_rx) = tokio::sync::mpsc::channel::<World>(10);
        let queue = Queue {
            sender: viewer_client_tx,
            receiver: client_viewer_rx,
        };

        let _handle = std::thread::spawn(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let mut client = Client::connect(
                    server_addr,
                    client::Config {
                        name: "cubes-tui-viewer".to_owned(),
                        is_blocking: false,
                        ..Default::default()
                    },
                )
                .await
                .unwrap();

                let mut receiver = client
                    .subscribe(
                        vec![Trigger::StepEvent],
                        Query::default()
                            .filter(query::Filter::Component(string::new_truncate("position")))
                            .map(query::Map::All)
                            .description(query::Description::NativeDescribed),
                    )
                    .await
                    .expect("subscription panic");

                while let Ok(Some(msg)) = receiver.recv().await {
                    // println!("got msg: {:?}", msg);
                    match msg {
                        Message::QueryResponse(product) => {
                            let world = product.into();
                            client_viewer_tx.send(world).await.unwrap();
                        }
                        Message::SubscribeResponse(id) => {
                            //
                            ()
                        }
                        _ => unimplemented!("{:?}", msg),
                    }
                }

                anyhow::Result::<()>::Ok(())
            })
            .unwrap();
        });

        Ok(App {
            queue,
            world: World::default(),
            poll_t: DEF_DUR,
        })
    }
}
impl App {
    pub fn paused(&self) -> bool {
        self.poll_t == PAUSE
    }

    pub fn play_pause(&mut self, prev_poll_t: &mut Duration) {
        if self.paused() {
            self.poll_t = *prev_poll_t;
        } else {
            *prev_poll_t = self.poll_t;
            self.poll_t = PAUSE;
        }
    }

    pub fn faster(&mut self, big: bool) {
        if !self.paused() {
            let div = if big { 2 } else { 5 };
            self.poll_t = self
                .poll_t
                .checked_sub(self.poll_t.checked_div(div).unwrap_or(DEF_DUR))
                .unwrap_or(DEF_DUR);
        }
    }
    pub fn slower(&mut self, big: bool) {
        if !self.paused() {
            let div = if big { 2 } else { 5 };
            self.poll_t = self
                .poll_t
                .checked_add(self.poll_t.checked_div(div).unwrap_or(DEF_DUR))
                .unwrap_or(DEF_DUR);
        }
    }

    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> anyhow::Result<()> {
        let mut prev_poll_t = self.poll_t;

        loop {
            terminal.draw(|f| ui::ui(f, self))?;

            // Wait up to `poll_t` for another event
            if event::poll(self.poll_t)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('j') | KeyCode::Down => self.slower(false),
                        KeyCode::Char('k') | KeyCode::Up => self.faster(false),
                        KeyCode::Char(' ') | KeyCode::Enter => self.play_pause(&mut prev_poll_t),
                        // KeyCode::Char('r') => self.restart(),
                        // KeyCode::Char('n' | 'l') | KeyCode::Right => self.next(),
                        // KeyCode::Char('p' | 'h') | KeyCode::Left => self.prev(),
                        // KeyCode::Char('R') | KeyCode::Backspace => *self = Self::default(),
                        _ => {}
                    }
                } else {
                    // resize and restart
                    // self.restart();
                }
            } else {
                // Timeout expired, updating state.

                if let Ok(world_update) = self.queue.receiver.try_recv() {
                    // println!("world_update: {:?}", world_update);
                    self.world = world_update;
                }

                // self.client.step(1).await?;
                // let product = self
                //     .client
                //     .query(Query::default().map(query::Map::All))
                //     .await?;
            }
        }

        Ok(())
    }
}
