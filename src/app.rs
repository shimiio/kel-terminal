use crate::terminal::Terminal;
use eframe::egui;
use std::sync::{Arc, Mutex};

pub struct KelApp {
    terminal: Arc<Mutex<Terminal>>,
}

impl KelApp {
    pub fn new(terminal: Arc<Mutex<Terminal>>) -> Self {
        Self { terminal }
    }
}

impl eframe::App for KelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let text = self.terminal.lock().unwrap().grid.render();
        ui.label(egui::RichText::new(text.to_string()).monospace());
    }
}
