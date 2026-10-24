use anyhow::{bail, Context, Result};
use clap::Parser;
use cogfabric::{chat_client, config::Config};
use std::{collections::VecDeque, io::Write, os::fd::RawFd, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    name = "cogfabric-chat",
    version,
    about = "Bounded, tool-free Cognitive Fabric chat"
)]
struct Args {
    /// Cognitive Fabric HTTP server base URL. Must be numeric loopback or localhost.
    #[arg(
        long,
        env = "COGFABRIC_CHAT_SERVER",
        default_value = "http://127.0.0.1:8234/v1"
    )]
    server: String,
    /// Private Cognitive Fabric settings directory.
    #[arg(long, env = "COGFABRIC_CHAT_HOME")]
    home: Option<PathBuf>,
    /// Maximum number of chat turns in this process.
    #[arg(long, env = "COGFABRIC_CHAT_MAX_TURNS", default_value_t = 16)]
    max_turns: usize,
    /// Reserved model output tokens per turn.
    #[arg(long, env = "COGFABRIC_CHAT_MAX_TOKENS", default_value_t = 256)]
    max_tokens: usize,
    /// Maximum wall time for the interactive session in seconds.
    #[arg(long, env = "COGFABRIC_CHAT_SESSION_SECONDS", default_value_t = 3600, value_parser = clap::value_parser!(u64).range(1..=86_400))]
    session_seconds: u64,
    /// Served chat model alias (collective aliases do not support multi-turn chat).
    #[arg(long, env = "COGFABRIC_CHAT_MODEL", default_value = "cogfabric-agent")]
    model: String,
    /// Environment variable containing the server API key; never pass a key value.
    #[arg(
        long,
        env = "COGFABRIC_CHAT_API_KEY_ENV",
        default_value = "COGFABRIC_SERVER_API_KEY"
    )]
    api_key_env: String,
    /// Per-request timeout; defaults to saved configuration, capped at 120 seconds.
    #[arg(long, env = "COGFABRIC_CHAT_TIMEOUT_SECONDS")]
    timeout_seconds: Option<u64>,
    /// Generation temperature; defaults to the saved configuration.
    #[arg(long, env = "COGFABRIC_CHAT_TEMPERATURE")]
    temperature: Option<f64>,
    #[arg(
        long,
        env = "COGFABRIC_CHAT_MAX_SESSION_USER_BYTES",
        default_value_t = 32768
    )]
    max_session_user_bytes: usize,
    #[arg(
        long,
        env = "COGFABRIC_CHAT_MAX_OUTPUT_RESERVATION",
        default_value_t = 4096
    )]
    max_output_reservation: usize,
    #[arg(long, env = "COGFABRIC_CHAT_MAX_MESSAGE_BYTES", default_value_t = 8192)]
    max_message_bytes: usize,
    #[arg(
        long,
        env = "COGFABRIC_CHAT_MAX_TRANSCRIPT_BYTES",
        default_value_t = 32768
    )]
    max_transcript_bytes: usize,
    #[arg(long, env = "COGFABRIC_CHAT_MAX_MESSAGES", default_value_t = 32)]
    max_messages: usize,
    #[arg(
        long,
        env = "COGFABRIC_CHAT_MAX_HTTP_RESPONSE_BYTES",
        default_value_t = 262144
    )]
    max_http_response_bytes: usize,
}

enum InputLine {
    End,
    Text(String),
    TooLong,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("cogfabric-chat: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();
    if !(1..=1024).contains(&args.max_turns)
        || !(1..=16384).contains(&args.max_tokens)
        || !(1..=16 * 1024 * 1024).contains(&args.max_session_user_bytes)
        || !(1..=1_048_576).contains(&args.max_output_reservation)
        || args
            .max_turns
            .checked_mul(args.max_tokens)
            .is_none_or(|total| total > args.max_output_reservation)
    {
        bail!("Invalid chat session limits or total reserved output budget exceeded")
    }
    let limits = chat_client::Limits {
        http_response_bytes: args.max_http_response_bytes,
        transcript_bytes: args.max_transcript_bytes,
        message_bytes: args.max_message_bytes,
        messages: args.max_messages,
    };
    limits.validate()?;
    let config = Config::load(args.home.clone(), None)?;
    let timeout = Duration::from_secs(
        args.timeout_seconds
            .unwrap_or(config.timeout_seconds.min(120)),
    );
    let temperature = args.temperature.unwrap_or(config.temperature);
    if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
        bail!("Chat temperature must be finite and between 0 and 2")
    }
    let client = chat_client::ClientApi::with_options(
        &args.server,
        timeout,
        args.max_tokens,
        chat_client::Options {
            model: args.model.clone(),
            api_key_env: args.api_key_env.clone(),
            limits: limits.clone(),
        },
    )?;
    match tokio::time::timeout(
        Duration::from_secs(args.session_seconds),
        run_session(client, temperature, &args, &limits),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            println!("\nSession time limit reached.");
            Ok(())
        }
    }
}

async fn run_session(
    client: chat_client::ClientApi,
    temperature: f64,
    args: &Args,
    limits: &chat_client::Limits,
) -> Result<()> {
    let mut input = NonblockingInput::new()?;
    let mut transcript = Vec::<chat_client::Message>::new();
    let mut total_user_bytes = 0usize;
    let mut turns = 0usize;

    println!(
        "Cognitive Fabric chat (model: {}). Type /help for commands.",
        args.model
    );
    loop {
        print!("you> ");
        std::io::stdout().flush()?;
        let line = tokio::select! {
            result = read_bounded_line(&mut input, limits.message_bytes) => result?,
            _ = tokio::signal::ctrl_c() => {
                println!("\nGoodbye.");
                return Ok(());
            }
        };
        let text = match line {
            InputLine::End => {
                println!();
                return Ok(());
            }
            InputLine::TooLong => {
                println!(
                    "Input is too long; one message is limited to {} bytes.",
                    limits.message_bytes
                );
                continue;
            }
            InputLine::Text(text) => text,
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        match text {
            "/quit" | "/exit" => {
                println!("Goodbye.");
                return Ok(());
            }
            "/help" => {
                println!("Commands: /help, /reset, /quit. Each chat turn is text-only; no tools are advertised or run.");
                continue;
            }
            "/reset" => {
                transcript.clear();
                println!("Conversation context cleared. Session input and token budgets remain in effect.");
                continue;
            }
            _ => {}
        }
        if turns >= args.max_turns {
            println!("Session turn limit reached.");
            return Ok(());
        }
        let input_bytes = text.len();
        if input_bytes > limits.message_bytes
            || total_user_bytes.saturating_add(input_bytes) > args.max_session_user_bytes
        {
            println!(
                "Session input limit reached ({} total user bytes).",
                args.max_session_user_bytes
            );
            return Ok(());
        }

        total_user_bytes += input_bytes;
        turns += 1;
        transcript.push(chatfabric_user_message(text));
        trim_transcript(&mut transcript, limits)?;

        let completion = tokio::select! {
            result = client.complete(&transcript, temperature) => Some(result),
            _ = tokio::signal::ctrl_c() => None,
        };
        let Some(completion) = completion else {
            println!("\nGoodbye.");
            return Ok(());
        };
        match completion {
            Ok(reply) => {
                println!("assistant> {}", safe_terminal_text(&reply));
                transcript.push(chatfabric_assistant_message(&reply));
                trim_transcript(&mut transcript, limits)?;
            }
            Err(error) => {
                transcript.pop();
                eprintln!("chat request failed: {error}");
            }
        }
    }
}

/// Prevent untrusted model text from sending terminal control and bidi commands.
fn safe_terminal_text(text: &str) -> String {
    use std::fmt::Write as _;
    let mut safe = String::with_capacity(text.len());
    for character in text.chars() {
        let code = character as u32;
        let bidi_control = matches!(code, 0x202A..=0x202E | 0x2066..=0x2069);
        if character == '\n' || character == '\t' {
            safe.push(character);
        } else if character.is_control() || bidi_control {
            let _ = write!(safe, "\\u{{{code:x}}}");
        } else {
            safe.push(character);
        }
    }
    safe
}

fn chatfabric_user_message(content: &str) -> chat_client::Message {
    chat_client::Message {
        role: chat_client::Role::User,
        content: content.to_owned(),
    }
}

fn chatfabric_assistant_message(content: &str) -> chat_client::Message {
    chat_client::Message {
        role: chat_client::Role::Assistant,
        content: content.to_owned(),
    }
}

/// Retains the newest complete exchanges and never splits a user/assistant pair.
fn trim_transcript(
    transcript: &mut Vec<chat_client::Message>,
    limits: &chat_client::Limits,
) -> Result<()> {
    while transcript.len() > limits.messages
        || serde_json::to_vec(transcript)?.len() > limits.transcript_bytes
    {
        if transcript.len() < 3
            || !matches!(
                transcript.first().map(|message| message.role),
                Some(chat_client::Role::User)
            )
            || !matches!(
                transcript.get(1).map(|message| message.role),
                Some(chat_client::Role::Assistant)
            )
        {
            bail!("The current chat exchange exceeds the transcript limit; use /reset to continue")
        }
        transcript.drain(..2);
    }
    Ok(())
}

/// Reads one UTF-8 line while keeping memory use bounded even for pasted input.
/// Nonblocking descriptor reads keep Ctrl-C and session expiry responsive even
/// when no newline is entered. Original descriptor flags are restored on drop.
struct NonblockingInput {
    fd: RawFd,
    original_flags: libc::c_int,
    pending: VecDeque<u8>,
    eof: bool,
}

impl NonblockingInput {
    fn new() -> Result<Self> {
        let fd = libc::STDIN_FILENO;
        // SAFETY: fcntl reads and updates flags on the process's standard input.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(std::io::Error::last_os_error())
                .context("Could not inspect standard input");
        }
        // SAFETY: only O_NONBLOCK is added; Drop restores the original flags.
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error())
                .context("Could not configure standard input");
        }
        Ok(Self {
            fd,
            original_flags: flags,
            pending: VecDeque::new(),
            eof: false,
        })
    }

    async fn next_byte(&mut self) -> Result<Option<u8>> {
        loop {
            if let Some(byte) = self.pending.pop_front() {
                return Ok(Some(byte));
            }
            if self.eof {
                return Ok(None);
            }
            let mut chunk = [0u8; 1024];
            // SAFETY: chunk is writable for its full length and fd remains open
            // for the duration of this client process.
            let count = unsafe { libc::read(self.fd, chunk.as_mut_ptr().cast(), chunk.len()) };
            if count > 0 {
                self.pending.extend(&chunk[..count as usize]);
                tokio::task::yield_now().await;
                continue;
            }
            if count == 0 {
                self.eof = true;
                return Ok(None);
            }
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            if error.kind() == std::io::ErrorKind::WouldBlock {
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }
            return Err(error).context("Could not read standard input");
        }
    }
}

impl Drop for NonblockingInput {
    fn drop(&mut self) {
        // SAFETY: restore the exact file status flags captured in new().
        let _ = unsafe { libc::fcntl(self.fd, libc::F_SETFL, self.original_flags) };
    }
}

async fn read_bounded_line(reader: &mut NonblockingInput, maximum: usize) -> Result<InputLine> {
    let mut bytes = Vec::with_capacity(maximum.min(1024));
    let mut too_long = false;
    let mut saw_bytes = false;
    loop {
        let Some(byte) = reader.next_byte().await? else {
            if !saw_bytes {
                return Ok(InputLine::End);
            }
            break;
        };
        if byte == b'\n' {
            break;
        }
        saw_bytes = true;
        if !too_long {
            if bytes.len() == maximum {
                too_long = true;
                bytes.clear();
            } else {
                bytes.push(byte);
            }
        }
    }
    if too_long {
        return Ok(InputLine::TooLong);
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    let text = String::from_utf8(bytes).context("Input must be valid UTF-8")?;
    Ok(InputLine::Text(text))
}
