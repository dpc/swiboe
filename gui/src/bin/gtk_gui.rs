#![cfg(not(test))]
#![feature(scoped)]

extern crate cairo;
extern crate gdk;
extern crate glib;
extern crate gtk;
extern crate serde;
extern crate switchboard;
extern crate switchboard_gtk_gui;
extern crate time;

use cairo::Context;
use cairo::enums::{FontSlant, FontWeight};
use gtk::signal;
use gtk::traits::*;
use serde::json;
use std::cell::{RefCell, Cell};
use std::clone::Clone;
use std::cmp;
use std::convert;
use std::f64::consts::PI;
use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::path;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{RwLock, Arc};
use std::thread;
use switchboard::client;
use switchboard::ipc;
use switchboard::plugin_buffer;
use switchboard_gtk_gui::buffer_view_widget;
use switchboard_gtk_gui::buffer_views;
use switchboard_gtk_gui::command::GuiCommand;

thread_local!(
    static GLOBAL: RefCell<Option<(buffer_view_widget::BufferViewWidget)>> = RefCell::new(None)
);

struct SwitchboardGtkGui {
    // NOCOM(#sirver): Is the Arc needed?
    buffer_views: Arc<RwLock<buffer_views::BufferViews>>,
    gui_id: String,
    gui_commands: mpsc::Receiver<GuiCommand>,
}

impl SwitchboardGtkGui {
    fn new(client: &client::Client) -> Self {

        // NOCOM(#sirver): use a uuid
        let gui_id: String = "blumbaquatsch".into();

        let (tx, rx) = mpsc::channel();
        let mut gui = SwitchboardGtkGui {
            buffer_views: buffer_views::BufferViews::new(&gui_id, tx.clone(), &client),
            gui_id: gui_id.clone(),
            gui_commands: rx,
        };

        let window = gtk::Window::new(gtk::WindowType::TopLevel).unwrap();
        window.set_title("Switchboard");
        window.set_window_position(gtk::WindowPosition::Center);
        window.set_default_size(800, 600);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0).unwrap();


        let cursor_id = {
            let mut buffer_views = gui.buffer_views.write().unwrap();
            let buffer_view = buffer_views.get_or_create(0);
            let cursor_id = buffer_view.cursor.id().to_string();
            cursor_id
        };

        // NOCOM(#sirver): rename BufferView to Editor(View)?
        let buffer_view_widget = buffer_view_widget::BufferViewWidget::new(
            gui.buffer_views.clone(),
        );
        vbox.pack_start(buffer_view_widget.widget(), true, true, 0);
        GLOBAL.with(|global| {
            *global.borrow_mut() = Some(buffer_view_widget)
        });

        let tx_clone = tx.clone();
        window.connect_delete_event(move |_, _| {
            tx_clone.send(GuiCommand::Quit).unwrap();
            gtk::main_quit();
            signal::Inhibit(true)
        });
        window.add(&vbox);

        // let drawing_area_clone = drawing_area.clone();
        let buffers_clone = gui.buffer_views.clone();
        let rpc_caller = client.new_rpc_caller();
        window.connect_key_press_event(move |_, key| {
            // println!("#sirver key: {:#?}", key);
            let state = (*key).state;
            println!("#sirver state: {:#?}", state);
            // if state.contains(gdk::ModifierType::from_bits_truncate(::utils::META_KEY)) {
                // let keyval = (*key).keyval;
                println!("#sirver (*key)._type: {:#?}", (*key)._type);
                if let Some(name_str) = gdk::keyval_name(key.keyval) {
                    println!("#sirver name_str: {:#?}", name_str);
                    println!("#sirver keypress: {}", time::precise_time_ns());
                    match &name_str as &str {
                        "F2" =>  {
                            let rpc = rpc_caller.call("buffer.open", &plugin_buffer::OpenRequest {
                                uri: "file:///Users/sirver/Desktop/Programming/rust/Switchboard/src/client.rs".into(),
                            });
                            let b: plugin_buffer::OpenResponse = rpc.wait_for().unwrap();
                            println!("#sirver b: {:#?}", b);
                        }
                        "Up" => {
                            rpc_caller.call("gui.buffer_view.move_cursor", &buffer_views::MoveCursorRequest {
                                cursor_id: cursor_id.clone(),
                                delta: buffer_views::Position { line_index: -1, column_index: 0, },
                            });
                        },
                        "Down" => {
                            rpc_caller.call("gui.buffer_view.move_cursor", &buffer_views::MoveCursorRequest {
                                cursor_id: cursor_id.clone(),
                                delta: buffer_views::Position { line_index: 1, column_index: 0, },
                            });
                        }
                        "Left" => {
                            rpc_caller.call("gui.buffer_view.move_cursor", &buffer_views::MoveCursorRequest {
                                cursor_id: cursor_id.clone(),
                                delta: buffer_views::Position { line_index: 0, column_index: -1, },
                            });
                        },
                        "Right" => {
                            rpc_caller.call("gui.buffer_view.move_cursor", &buffer_views::MoveCursorRequest {
                                cursor_id: cursor_id.clone(),
                                delta: buffer_views::Position { line_index: 0, column_index: 1, },
                            });
                        },
                        _ => (),
                    // // if let Some(button) = shortcuts.get(&name_str) {
                        // // button.clicked();
                        // return signal::Inhibit(true);
                    }
                }

            // }
            signal::Inhibit(false)
        });


        // NOCOM(#sirver): bring back
        // glib::source::timeout_add(100, move || {
            // while let Some(msg) = client.poll() {
            // }
            // glib::source::Continue(true)
        // });

        window.show_all();
        gui
    }
}

fn main() {
    gtk::init().unwrap_or_else(|_| panic!("Failed to initialize GTK."));

    let client = client::Client::connect(path::Path::new("/tmp/sb.socket"));
    let mut switchboard = SwitchboardGtkGui::new(&client);

    let guard = thread::scoped(move || {
        while let Ok(command) = switchboard.gui_commands.recv() {
            match command {
                GuiCommand::Quit => break,
                GuiCommand::Redraw => {
                    glib::idle_add(|| {
                        GLOBAL.with(|global| {
                            if let Some(ref widget) = *global.borrow() {
                                widget.widget().queue_draw();
                            }
                        });
                        glib::source::Continue(false)
                    });
                }
            }
        }
    });

    gtk::main();
}
