mod cli;

use crate::cli::Opt;
use jack::{Client, Control, MidiIn, Port, ProcessHandler, ProcessScope};
use nom_midi::{MidiEvent, MidiEventType};
use std::{
    error::Error as StdError,
    io::{self, BufRead},
    str::FromStr,
    sync::atomic::{AtomicI8, Ordering},
    fmt
};
use structopt::StructOpt;

/// Main programm runner.
fn run(opts: Opt) -> Result<(), Box<dyn StdError>> {
    let (client, status) = Client::new(&opts.jack_name, jack::ClientOptions::NO_START_SERVER)?;
    log::info!("name: {}", client.name());
    let ports = Ports::setup(&client)?;
    let async_client = client.activate_async((), ports)?;
    let (_tx, rx) = std::sync::mpsc::channel::<()>();
    rx.recv(); // block forever
    Ok(())
}

struct Ports {
    sink: Port<MidiIn>,
}

impl Ports {
    /// Our constructor. Here we setup the ports we want and store them in our jack state object.
    fn setup(client: &Client) -> Result<Self, Box<dyn StdError>> {
        let sink = client.register_port("sink", MidiIn)?;

        Ok(Ports { sink })
    }
}

impl ProcessHandler for Ports {
    fn process(&mut self, _client: &Client, process_scope: &ProcessScope) -> Control {
        // process midi
        for raw_midi in self.sink.iter(process_scope) {
            match nom_midi::parser::parse_midi_event(raw_midi.bytes) {
                Ok((_, evt)) => {
                    log::info!("received {:?} at {} (raw: {:?})", evt, raw_midi.time, RawMidi(&raw_midi.bytes));
                }
                Err(_) => {
                    log::info!(
                        "unparseable midi event {:?} at {}",
                        RawMidi(&raw_midi.bytes),
                        raw_midi.time
                    );
                }
            }
        }
        Control::Continue
    }
}

/// We want to either increment or decrement the volume
#[derive(Debug, Clone)]
pub enum Msg {
    /// Volume up
    Up,
    /// Volume down
    Down,
}

impl FromStr for Msg {
    type Err = &'static str;

    fn from_str(msg: &str) -> Result<Self, Self::Err> {
        if msg.eq_ignore_ascii_case("up") {
            Ok(Msg::Up)
        } else if msg.eq_ignore_ascii_case("down") {
            Ok(Msg::Down)
        } else {
            Err("unrecognised command")
        }
    }
}

// boilerplate

/// Wrap the run method so we can pass it command line args, setup logging, and handle errors
/// gracefully.
fn main() {
    let opts = Opt::from_args();
    setup_logger(opts.verbosity);
    if let Err(err) = run(opts) {
        log::error!("{}", err);
        let mut e = &*err;
        while let Some(err) = e.source() {
            log::error!("caused by {}", err);
            e = err;
        }
    }
}

/// Make the logger match our verbosity. This is custom because we don't want to see all messages
/// from other packages, only `jack-volume`.
fn setup_logger(verbosity: u32) {
    use log::LevelFilter;
    let level = match verbosity {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    pretty_env_logger::formatted_timed_builder()
        .filter(None, LevelFilter::Warn)
        .filter(Some("jack_midi_sink"), level)
        .init()
}

struct RawMidi<'a>(&'a [u8]);

impl fmt::Debug for RawMidi<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;
        let mut iter = self.0.iter();
        if let Some(byte) = iter.next() {
            write!(f, "{:x}", byte)?;
        }
        for byte in iter {
            write!(f, ", {:x}", byte)?;
        }
        write!(f, "]")
    }
}
