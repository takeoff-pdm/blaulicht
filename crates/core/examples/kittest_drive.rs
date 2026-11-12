//! Drive the real desk UI without a window: the engine plus `BlaulichtApp` in an
//! `egui_kittest` harness, rendered offscreen with wgpu (Metal on macOS).
//!
//! Commands come in over a Unix socket, one per connection, so an agent can poke
//! the running UI from one-off shell commands (`scripts/kt`). See
//! `tools/blenv/KITTEST.md`.
//!
//!   cargo run -p blaulicht-core --example kittest_drive -- \
//!       --socket /tmp/kt.sock [--layout desktop] [--config-file crates/core/config.toml]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use blaulicht_engine::cli::EngineArgs;
use blaulicht_engine::{Engine, EngineOptions};
use blaulicht_ui_egui::app::BlaulichtApp;
use blaulicht_ui_egui::cli::UiArgs;
use clap::Parser;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

#[derive(Parser, Debug)]
struct Args {
    /// Unix socket to listen on for commands.
    #[arg(long, default_value = "/tmp/blaulicht-kittest.sock")]
    socket: PathBuf,
    /// Harness size in points (= pixels; the app pins pixels_per_point to 1).
    #[arg(long, default_value_t = 1280.0)]
    width: f32,
    #[arg(long, default_value_t = 800.0)]
    height: f32,
    #[command(flatten)]
    ui: UiArgs,
    #[command(flatten)]
    engine: EngineArgs,
}

/// Frames to run after every command. The app always requests a repaint, so
/// `Harness::run` never settles; real time also passes so engine events arrive.
const SETTLE_FRAMES: usize = 6;
const FRAME: Duration = Duration::from_millis(16);

fn settle(h: &mut Harness<'_, BlaulichtApp>, frames: usize) {
    for _ in 0..frames {
        let start = Instant::now();
        h.step();
        if let Some(rest) = FRAME.checked_sub(start.elapsed()) {
            std::thread::sleep(rest);
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber_init();
    let args = Args::parse();

    let Some(mut engine) = Engine::start(EngineOptions {
        config_file: args.engine.config_file(),
        attach_desk: true,
    })?
    else {
        anyhow::bail!("config file was missing and has been created; run again");
    };
    let desk = engine.take_desk().expect("engine started with a desk client");
    let config_dir = engine.config_file().parent().map(|d| d.to_path_buf());

    let layout = args.ui.layout();
    let ui = args.ui;
    let mut h = Harness::builder()
        .with_size(egui::vec2(args.width, args.height))
        .with_pixels_per_point(1.0)
        .with_step_dt(FRAME.as_secs_f32())
        .wgpu()
        .build_eframe(move |cc| {
            BlaulichtApp::new(
                cc,
                desk.into(),
                None,
                layout,
                ui.showfile_home,
                config_dir,
                ui.external_screens,
            )
        });
    settle(&mut h, 30);

    let _ = std::fs::remove_file(&args.socket);
    let listener = UnixListener::bind(&args.socket)?;
    eprintln!("kittest_drive: ready on {}", args.socket.display());

    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        let line = line.trim();
        let reply = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            command(&mut h, line)
        })) {
            Ok(Ok(Some(text))) => text,
            Ok(Ok(None)) => break,
            Ok(Err(e)) => format!("error: {e}"),
            Err(_) => "error: command panicked (see driver stderr)".into(),
        };
        let _ = writeln!(stream, "{reply}");
    }
    let _ = std::fs::remove_file(&args.socket);
    drop(h);
    engine.shutdown();
    Ok(())
}

/// Finds a node by exact label, else by label substring; errors if ambiguous.
fn find<'h>(
    h: &'h Harness<'_, BlaulichtApp>,
    label: &'h str,
) -> anyhow::Result<egui_kittest::Node<'h>> {
    let mut exact: Vec<_> = h.query_all_by_label(label).collect();
    if exact.is_empty() {
        exact = h.query_all_by_label_contains(label).collect();
    }
    match exact.len() {
        0 => anyhow::bail!("no node labelled {label:?} (try `tree {label}`)"),
        1 => Ok(exact.remove(0)),
        n => {
            // Several matches: prefer the first visible one, but say so.
            eprintln!("kittest_drive: {n} nodes match {label:?}, using the first");
            Ok(exact.remove(0))
        }
    }
}

fn pointer_click(h: &mut Harness<'_, BlaulichtApp>, p: egui::Pos2, button: egui::PointerButton) {
    h.hover_at(p);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button, pressed, modifiers: Default::default() });
        h.step();
    }
    settle(h, SETTLE_FRAMES);
}

fn pos(a: &str, b: &str) -> anyhow::Result<egui::Pos2> {
    Ok(egui::pos2(a.parse()?, b.parse()?))
}

fn command(h: &mut Harness<'_, BlaulichtApp>, line: &str) -> anyhow::Result<Option<String>> {
    let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
    let rest = rest.trim();
    let words: Vec<&str> = rest.split_whitespace().collect();
    let out = match cmd {
        "quit" => return Ok(None),
        // Clicks go through the real pointer path (hover, press, release in
        // separate frames). kittest's `Node::click` sends press and release in one
        // frame, which popups such as ComboBox ignore.
        "click" | "rclick" => {
            let r = find(h, rest)?.rect();
            let button = if cmd == "click" {
                egui::PointerButton::Primary
            } else {
                egui::PointerButton::Secondary
            };
            pointer_click(h, r.center(), button);
            format!("clicked {rest:?} at {:.0},{:.0}", r.center().x, r.center().y)
        }
        "click_at" => {
            let p = pos(words[0], words[1])?;
            pointer_click(h, p, egui::PointerButton::Primary);
            format!("clicked at {:.0},{:.0}", p.x, p.y)
        }
        "drag" => {
            let (a, b) = (pos(words[0], words[1])?, pos(words[2], words[3])?);
            h.hover_at(a);
            h.step();
            h.drag_at(a);
            h.step();
            for i in 1..=10 {
                h.hover_at(a + (b - a) * (i as f32 / 10.0));
                h.step();
            }
            h.drop_at(b);
            settle(h, SETTLE_FRAMES);
            format!("dragged {a:?} -> {b:?}")
        }
        "scroll" => {
            // scroll x y dy: wheel at a point (negative dy scrolls down).
            let p = pos(words[0], words[1])?;
            let dy: f32 = words[2].parse()?;
            h.hover_at(p);
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                modifiers: Default::default(),
                phase: egui::TouchPhase::Move,
            });
            settle(h, SETTLE_FRAMES);
            format!("scrolled {dy} at {p:?}")
        }
        "type" => {
            h.event(egui::Event::Text(rest.to_owned()));
            settle(h, SETTLE_FRAMES);
            format!("typed {rest:?}")
        }
        "key" => {
            // key Enter | key ctrl+S | key shift+Tab
            let mut mods = egui::Modifiers::NONE;
            let mut key = None;
            for part in rest.split('+') {
                match part.to_ascii_lowercase().as_str() {
                    "ctrl" => mods.ctrl = true,
                    "cmd" => mods = mods | egui::Modifiers::COMMAND,
                    "shift" => mods.shift = true,
                    "alt" => mods.alt = true,
                    _ => key = egui::Key::from_name(part),
                }
            }
            let key = key.ok_or_else(|| anyhow::anyhow!("unknown key in {rest:?}"))?;
            h.key_press_modifiers(mods, key);
            settle(h, SETTLE_FRAMES);
            format!("pressed {rest}")
        }
        "hover" => {
            let p = pos(words[0], words[1])?;
            h.hover_at(p);
            settle(h, SETTLE_FRAMES);
            format!("hovering {p:?}")
        }
        "wait" => {
            let ms: u64 = rest.parse().unwrap_or(500);
            settle(h, (ms / 16).max(1) as usize);
            format!("waited {ms} ms")
        }
        "size" => {
            h.set_size(egui::vec2(words[0].parse()?, words[1].parse()?));
            settle(h, SETTLE_FRAMES);
            format!("size {} x {}", words[0], words[1])
        }
        "shot" => {
            let path = if rest.is_empty() { "/tmp/kittest.png" } else { rest };
            let img = h.render().map_err(|e| anyhow::anyhow!(e))?;
            img.save(path)?;
            format!("saved {path} ({}x{})", img.width(), img.height())
        }
        "tree" => {
            // tree [filter]: every labelled node as `role "label" @ x,y wxh`.
            let filter = rest.to_lowercase();
            let mut lines = Vec::new();
            for node in h.root().children_recursive() {
                let ak = node.accesskit_node();
                let label = ak.label().unwrap_or_default();
                let value = ak.value().unwrap_or_default();
                let input = matches!(
                    ak.role(),
                    egui::accesskit::Role::TextInput | egui::accesskit::Role::MultilineTextInput
                );
                // TextRuns repeat their Label's text.
                if ak.role() == egui::accesskit::Role::TextRun
                    || (label.is_empty() && value.is_empty() && !input)
                {
                    continue;
                }
                if !filter.is_empty()
                    && !label.to_lowercase().contains(&filter)
                    && !value.to_lowercase().contains(&filter)
                {
                    continue;
                }
                let r = node.rect();
                let val = if value.is_empty() { String::new() } else { format!(" = {value:?}") };
                lines.push(format!(
                    "{:?} {label:?}{val} @ {:.0},{:.0} {:.0}x{:.0}",
                    ak.role(),
                    r.min.x,
                    r.min.y,
                    r.width(),
                    r.height()
                ));
            }
            if lines.is_empty() {
                "(no matching nodes)".into()
            } else {
                lines.join("\n")
            }
        }
        "" | "help" => "commands: click LABEL | rclick LABEL | click_at X Y | drag X1 Y1 X2 Y2 | \
             scroll X Y DY | hover X Y | type TEXT | key [ctrl+|cmd+|shift+|alt+]KEY | \
             wait MS | size W H | shot [PATH] | tree [FILTER] | quit"
            .into(),
        other => anyhow::bail!("unknown command {other:?} (try help)"),
    };
    Ok(Some(out))
}

fn tracing_subscriber_init() {
    // The engine logs through `tracing`; core's main sets this up the same way
    // if a subscriber is a dependency. Keep the driver quiet otherwise.
}
