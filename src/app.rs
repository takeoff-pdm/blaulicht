use cpal::Device;
use serde::Deserialize;

#[derive(Clone)]
pub enum FromFrontend {
    Reload,
    SelectInputDevice(Option<Device>),
    SelectSerialDevice(Option<String>),
}

// We derive Deserialize/Serialize so we can persist app state on shutdown.
// #[derive(serde::Deserialize, serde::Serialize)]
// #[serde(default)] // if we add new fields, give them default values when deserializing old state
// pub struct BlaulichtApp {
//     log: Vec<String>,
//
//     // Example stuff:
//     label: String,
//
//     #[serde(skip)]
//     value: f32,
//
//     #[serde(skip)]
//     beat: bool,
//
//     #[serde(skip)]
//     beat_algo: bool,
//
//     #[serde(skip)]
//     beat_algo_time: Instant,
//
//     #[serde(skip)]
//     signal_in: Receiver<Signal>,
//
//     #[serde(skip)]
//     sys_out: Receiver<SystemMessage>,
//
//     //
//     // Audio.
//     //
//     #[serde(skip)]
//     audio_devices: Vec<(HostId, Device)>,
//
//     #[serde(skip)]
//     selected_audio_device: Option<Device>,
//
//     //
//     // Serial.
//     //
//     #[serde(skip)]
//     serial_devices: Vec<SerialPortInfo>,
//
//     #[serde(skip)]
//     selected_serial_device: Option<SerialPortInfo>,
//
//     // #[serde(skip)]
//     // dmx_control_sender: Sender<DMXControl>,
//     #[serde(skip)]
//     config: config::Config,
//
//     #[serde(skip)]
//     to_audio: Sender<FromFrontend>,
//
//     #[serde(skip)]
//     audio_thread_control_signal: Arc<AtomicU8>,
// }
//
// impl Default for BlaulichtApp {
//     fn default() -> Self {
//         let (_, receiver) = crossbeam_channel::unbounded();
//         let (sender, _) = crossbeam_channel::unbounded();
//         let (_, recv_sys) = crossbeam_channel::unbounded();
//         let mut audio_thread_control_signal = Arc::new(AtomicU8::new(0));
//
//         Self {
//             log: vec![],
//             // Example stuff:
//             label: "Hello World!".to_owned(),
//             value: 2.7,
//             beat: false,
//             beat_algo: false,
//             beat_algo_time: Instant::now(),
//             signal_in: receiver,
//
//             // Audio.
//             audio_devices: vec![],
//             selected_audio_device: None,
//
//             // Serial
//             serial_devices: vec![],
//             selected_serial_device: None,
//             // dmx_control_sender: sender,
//
//             // Config
//             config: config::Config::default(),
//             to_audio: sender,
//             audio_thread_control_signal,
//
//             sys_out: recv_sys,
//         }
//     }
// }
//
// impl BlaulichtApp {
//     /// Called once before the first frame.
//     pub fn new(
//         cc: &eframe::CreationContext<'_>,
//         from_frontend: Sender<FromFrontend>,
//         audio_thread_control_signal: Arc<AtomicU8>,
//         signal_in: Receiver<Signal>,
//         sys_recv: Receiver<SystemMessage>,
//         config: config::Config,
//     ) -> Self {
//         // This is also where you can customize the look and feel of egui using
//         // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.
//
//         // Load previous app state (if any).
//         // Note that you must enable the `persistence` feature for this to work.
//         // if let Some(storage) = cc.storage {
//         //     print!("load last");
//         //     return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
//         // }
//
//         Self {
//             log: vec![],
//             label: "foo label".into(),
//             value: 0f32,
//             beat: false,
//             beat_algo: false,
//             beat_algo_time: Instant::now(),
//             signal_in,
//
//             audio_devices: vec![],
//             selected_audio_device: None,
//
//             serial_devices: vec![],
//             selected_serial_device: None,
//             to_audio: from_frontend,
//             // dmx_control_sender,
//             sys_out: sys_recv,
//             audio_thread_control_signal,
//             config,
//         }
//     }
//
//     fn select_audio_device(&mut self, device: Option<Device>) {
//         if self.selected_audio_device.is_some() {
//             while self.audio_thread_control_signal.load(Ordering::Relaxed)
//                 != AudioThreadControlSignal::DEAD
//             {
//                 self.audio_thread_control_signal
//                     .store(AudioThreadControlSignal::ABORT, Ordering::Relaxed);
//
//                 println!("Waiting for audio thread to die...");
//                 thread::sleep(Duration::from_millis(100));
//             }
//
//             self.audio_thread_control_signal
//                 .store(AudioThreadControlSignal::CONTINUE, Ordering::Relaxed);
//         }
//
//         println!("Audio thread dead.");
//
//         self.to_audio
//             .send(FromFrontend::SelectInputDevice(device))
//             .unwrap();
//
//         // todo: abort audio loop
//
//         println!("updated audio device in App");
//     }
// }
//
// impl eframe::App for BlaulichtApp {
//     /// Called by the frame work to save state before shutdown.
//     fn save(&mut self, storage: &mut dyn eframe::Storage) {
//         eframe::set_value(storage, eframe::APP_KEY, self);
//     }
//
//     /// Called each time the UI needs repainting, which may be many times per second.
//     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
//         // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
//         // For inspiration and more examples, go to https://emilk.github.io/egui
//
//         egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
//             // The top panel is often a good place for a menu bar:
//
//             egui::menu::bar(ui, |ui| {
//                 // NOTE: no File->Quit on web pages!
//                 let is_web = cfg!(target_arch = "wasm32");
//                 if !is_web {
//                     ui.menu_button("File", |ui| {
//                         if ui.button("Quit").clicked() {
//                             ctx.send_viewport_cmd(egui::ViewportCommand::Close);
//                         }
//                     });
//                     ui.add_space(16.0);
//                 }
//
//                 egui::widgets::global_theme_preference_buttons(ui);
//             });
//         });
//
//         egui::CentralPanel::default().show(ctx, |ui| {
//             ui.heading("Blaulicht");
//
//             for msg in self.log.iter() {
//                 ui.monospace(msg);
//             }
//
//             {
//                 let button_title = format!(
//                     "Select Serial Device: {}",
//                     self.selected_serial_device
//                         .as_ref()
//                         .map_or_else(|| "NONE".to_string(), |dev| { dev.port_name.to_string() })
//                 );
//
//                 ui.menu_button(button_title, |ui| {
//                     let all_devices = self.serial_devices.clone().into_iter().chain(
//                         self.config
//                             .extra_serial_paths
//                             .iter()
//                             .map(|dev| SerialPortInfo {
//                                 port_name: dev.to_string_lossy().into(),
//                                 port_type: serialport::SerialPortType::Unknown,
//                             }),
//                     );
//
//                     for dev in all_devices {
//                         if ui.button(dev.port_name.clone()).clicked() {
//                             // ctx.send_viewport_cmd(egui::ViewportCommand::Close);
//
//                             self.selected_serial_device = Some(dev.clone());
//
//                             // TODO: notify DMX thread...
//                             //
//                             //
//                             println!("update");
//
//                             // self.dmx_control_sender
//                             //     .send(DMXControl::ChangePort(Some(dev)))
//                             //     .unwrap();
//
//                             ctx.request_repaint();
//                         }
//                     }
//
//                     if ui.button("NONE").clicked() {
//                         self.selected_serial_device = None;
//
//                         // TODO: notify DMX thread...
//                         //
//                         println!("update none");
//
//                         // self.dmx_control_sender
//                         //     .send(DMXControl::ChangePort(None))
//                         //     .unwrap();
//
//                         ctx.request_repaint();
//                     }
//                 });
//             }
//
//             ui.add_space(16.0);
//
//             // Second button
//
//             {
//                 let button_title = format!(
//                     "Select Audio Device: {}",
//                     self.selected_audio_device
//                         .as_ref()
//                         .map_or_else(|| "NONE".to_string(), |dev| { dev.name().unwrap() })
//                 );
//
//                 ui.menu_button(button_title, |ui| {
//                     // TODO: this is horrible, it is soo slow.
//                     for dev in self.audio_devices.clone().into_iter() {
//                         if ui
//                             .button(format!("{}:{}", dev.0.name(), dev.1.name().unwrap()))
//                             .clicked()
//                         {
//                             // ctx.send_viewport_cmd(egui::ViewportCommand::Close);
//
//                             // self.selected_audio_device = Some(dev.1.clone());
//                             //
//                             // // TODO: notify DMX thread...
//                             // //
//                             // //
//                             // println!("update");
//                             // self.to_audio
//                             //     .send(FromFrontend::SelectInputDevice(Some(dev.1.clone())))
//                             //     .unwrap();
//                             //
//                             self.select_audio_device(Some(dev.1.clone()));
//
//                             // self.dmx_control_sender
//                             //     .send(DMXControl::ChangePort(Some(dev)))
//                             //     .unwrap();
//
//                             ctx.request_repaint();
//                         }
//                     }
//
//                     if ui.button("NONE").clicked() {
//                         // self.selected_audio_device = None;
//
//                         // TODO: notify DMX thread...
//                         //
//                         // println!("update none");
//                         //
//                         // // self.dmx_control_sender
//                         // //     .send(DMXControl::ChangePort(None))
//                         // //     .unwrap();
//                         // self.to_audio
//                         //     .send(FromFrontend::SelectInputDevice(None))
//                         //     .unwrap();
//                         //
//
//                         self.select_audio_device(None);
//
//                         ctx.request_repaint();
//                     }
//                 });
//             }
//
//             if let Ok(sig) = self.signal_in.try_recv() {
//                 if self.beat_algo_time.elapsed().as_millis() > 10 {
//                     self.beat_algo = false;
//                 }
//
//                 match sig {
//                     Signal::Volume(new_vol) => {
//                         self.value = new_vol as f32;
//                     }
//                     Signal::Bass(_) => {}
//                     Signal::BeatVolume(v) => {
//                         self.beat = v > 0;
//                     }
//                     Signal::BeatAlgo(v) => {
//                         self.beat_algo = true;
//                         self.beat_algo_time = Instant::now();
//                     }
//                 }
//             }
//
//             match self.sys_out.try_recv() {
//                 Ok(SystemMessage::Log(msg)) => self.log.push(msg),
//                 Ok(SystemMessage::LoopSpeed(speed)) => {}
//                 Ok(SystemMessage::AudioDevicesView(devices)) => {
//                     self.audio_devices = devices;
//                 }
//                 Ok(SystemMessage::AudioSelected(dev)) => {
//                     self.selected_audio_device = dev;
//                 }
//                 Ok(SystemMessage::SerialDevicesView(devices)) => self.serial_devices = devices,
//                 Ok(SystemMessage::SerialSelected(dev)) => self.selected_serial_device = dev,
//                 Err(TryRecvError::Empty) => {}
//                 Err(TryRecvError::Disconnected) => panic!("a"),
//             }
//
//             ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
//                 ui.add(egui::Slider::new(&mut self.value, 0.0..=255.0).text("volume"));
//
//                 {
//                     let color = if self.beat {
//                         Color32::GREEN
//                     } else {
//                         Color32::BLACK
//                     };
//
//                     let radius = 10.0;
//                     let (rect, response) =
//                         ui.allocate_exact_size(Vec2::splat(radius * 2f32), egui::Sense::hover());
//                     if ui.is_rect_visible(rect) {
//                         let center = rect.center();
//                         let circle = Shape::circle_filled(center, radius, color);
//                         ui.painter().add(circle);
//                     }
//                 }
//
//                 {
//                     let color = if self.beat_algo {
//                         Color32::GREEN
//                     } else {
//                         Color32::BLACK
//                     };
//
//                     let radius = 10.0;
//                     let (rect, response) =
//                         ui.allocate_exact_size(Vec2::splat(radius * 2f32), egui::Sense::hover());
//                     if ui.is_rect_visible(rect) {
//                         let center = rect.center();
//                         let circle = Shape::circle_filled(center, radius, color);
//                         ui.painter().add(circle);
//                     }
//                 }
//             });
//
//             ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
//                 // powered_by_egui_and_eframe(ui);
//                 egui::warn_if_debug_build(ui);
//             });
//         });
//
//         ctx.request_repaint();
//     }
// }
