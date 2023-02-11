// SPDX-License-Identifier: GPL-3.0-or-later
use adw::prelude::*;
use gettextrs::gettext;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use crate::ShutterMode;

/// Magic factor: Consider a square with width = w, then inscribe a circle
/// inside it and then a square inside the circle. LAMBDA * w represents the
/// distance between the small and the big square, which is precisely the
/// translation we need for the record animation. Equals to (sqrt(2) - 1) / 2.
///
/// ```
/// assert!((LAMBDA - (2.0_f32.sqrt() / 2.0 - 0.5)).abs() < f32::EPSILON);
/// ```
const LAMBDA: f32 = 0.20710677;

mod imp {
    use super::*;

    use glib::Properties;
    use once_cell::sync::OnceCell;
    use std::cell::Cell;

    #[derive(Debug, Properties)]
    #[properties(wrapper_type = super::ShutterButton)]
    pub struct ShutterButton {
        #[property(get, set = Self::set_shutter_mode, explicit_notify, builder(ShutterMode::default()))]
        pub shutter_mode: Cell<ShutterMode>,
        #[property(get, set = Self::set_countdown, explicit_notify)]
        pub countdown: Cell<u32>,

        // TODO Remove this, we can query the value directly from the
        // animations.
        pub countdown_val: Cell<f64>,
        pub record_val: Cell<f64>,
        pub press_val: Cell<f64>,
        pub mode_val: Cell<f64>,

        pub countdown_ani: OnceCell<adw::TimedAnimation>,
        pub record_ani: OnceCell<adw::TimedAnimation>,
        pub press_ani: OnceCell<adw::TimedAnimation>,
        pub mode_ani: OnceCell<adw::TimedAnimation>,
    }

    impl Default for ShutterButton {
        fn default() -> Self {
            Self {
                shutter_mode: Default::default(),
                countdown: Cell::new(0),

                countdown_val: Cell::new(1.0),
                record_val: Cell::new(0.0),
                press_val: Cell::new(4.0),
                mode_val: Cell::new(1.0),

                countdown_ani: Default::default(),
                record_ani: Default::default(),
                press_ani: Default::default(),
                mode_ani: Default::default(),
            }
        }
    }

    impl ShutterButton {
        pub fn set_countdown(&self, countdown: u32) {
            if countdown != self.countdown.replace(countdown) {
                self.obj().notify("countdown")
            }
        }

        pub fn set_shutter_mode(&self, shutter_mode: ShutterMode) {
            let widget = self.obj();
            if shutter_mode != self.shutter_mode.replace(shutter_mode) {
                let from = self.mode_val.get();
                let record_from = self.record_val.get();
                match shutter_mode {
                    ShutterMode::Picture => {
                        widget.mode_ani().set_value_to(1.0);
                        widget.mode_ani().set_value_from(from);
                        widget.mode_ani().play();

                        widget.record_ani().set_value_from(record_from);
                        widget.record_ani().set_value_to(0.0);
                        widget.record_ani().play();
                    }
                    ShutterMode::Video => {
                        widget.mode_ani().set_value_to(0.0);
                        widget.mode_ani().set_value_from(from);
                        widget.mode_ani().play();

                        widget.record_ani().set_value_from(record_from);
                        widget.record_ani().set_value_to(0.0);
                        widget.record_ani().play();
                    }
                    ShutterMode::Recording => {
                        widget.mode_ani().set_value_to(0.0);
                        widget.mode_ani().set_value_from(from);
                        widget.mode_ani().play();

                        widget.record_ani().set_value_from(record_from);
                        widget.record_ani().set_value_to(1.0);
                        widget.record_ani().play();
                    }
                }

                widget.notify("shutter-mode");
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ShutterButton {
        const NAME: &'static str = "ShutterButton";
        type Type = super::ShutterButton;
        type ParentType = gtk::Button;
    }

    impl ObjectImpl for ShutterButton {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }

        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            Self::derived_property(self, id, pspec)
        }

        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            Self::derived_set_property(self, id, value, pspec)
        }

        fn constructed(&self) {
            self.parent_constructed();

            let widget = self.obj();

            widget.add_css_class("shutterbutton");
            widget.add_css_class("flat");
            widget.set_tooltip_text(Some(&gettext("Shutter Button")));

            if matches!(widget.shutter_mode(), ShutterMode::Video) {
                self.mode_val.set(0.0);
                widget.queue_draw();
            }

            // Initialize animations.
            let press_target =
                adw::CallbackAnimationTarget::new(glib::clone!(@weak widget => move |value| {
                    widget.imp().press_val.set(value);
                    widget.queue_draw();
                }));
            let press_ani = adw::TimedAnimation::new(&*widget, 4.0, 8.0, 125, press_target);
            self.press_ani.set(press_ani).unwrap();

            let mode_target =
                adw::CallbackAnimationTarget::new(glib::clone!(@weak widget => move |value| {
                    widget.imp().mode_val.set(value);
                    widget.queue_draw();
                }));
            let mode_ani = adw::TimedAnimation::new(&*widget, 1.0, 0.0, 250, mode_target);
            self.mode_ani.set(mode_ani).unwrap();

            let countdown_target =
                adw::CallbackAnimationTarget::new(glib::clone!(@weak widget => move |value| {
                    widget.imp().countdown_val.set(value);
                    widget.queue_draw();
                }));
            let countdown_ani = adw::TimedAnimation::new(&*widget, 1.0, 0.0, 250, countdown_target);
            // TODO Figure out what easing to use.
            countdown_ani.set_easing(adw::Easing::Linear);
            self.countdown_ani.set(countdown_ani).unwrap();

            let record_target =
                adw::CallbackAnimationTarget::new(glib::clone!(@weak widget => move |value| {
                    widget.imp().record_val.set(value);
                    widget.queue_draw();
                }));
            let record_ani = adw::TimedAnimation::new(&*widget, 0.0, 0.0, 250, record_target);
            self.record_ani.set(record_ani).unwrap();

            self.obj()
                .connect_state_flags_changed(move |obj, old_flags| {
                    if obj.state_flags().contains(gtk::StateFlags::ACTIVE)
                        && !old_flags.contains(gtk::StateFlags::ACTIVE)
                    {
                        let press_ani = obj.imp().press_ani.get().unwrap();
                        press_ani.set_value_to(8.0);
                        press_ani.play();
                    } else if !obj.state_flags().contains(gtk::StateFlags::ACTIVE)
                        && old_flags.contains(gtk::StateFlags::ACTIVE)
                    {
                        let press_ani = obj.imp().press_ani.get().unwrap();
                        press_ani.set_value_to(4.0);
                        press_ani.play();
                    }
                });
        }
    }

    impl WidgetImpl for ShutterButton {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();

            let width = widget.width();
            let height = widget.height();

            let size = width.min(height) as f32;
            let border_width = (size / 8.0).min(8.0);

            widget.draw_border(snapshot, size, border_width);
            widget.draw_play(snapshot, size, border_width);
        }
    }

    impl ButtonImpl for ShutterButton {}
}

glib::wrapper! {
    pub struct ShutterButton(ObjectSubclass<imp::ShutterButton>)
        @extends gtk::Widget, gtk::Button;
}

impl Default for ShutterButton {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ShutterButton {
    pub fn start_countdown(&self) {
        self.countdown_ani().set_value_to(0.0);
        self.countdown_ani().set_duration(self.countdown() * 1000);
        self.countdown_ani().play();
    }

    pub fn stop_countdown(&self) {
        self.countdown_ani().set_value_to(1.0);
        self.countdown_ani().set_duration(0);
        self.countdown_ani().play();
    }

    fn record_ani(&self) -> &adw::TimedAnimation {
        self.imp().record_ani.get().unwrap()
    }

    fn mode_ani(&self) -> &adw::TimedAnimation {
        self.imp().mode_ani.get().unwrap()
    }

    fn countdown_ani(&self) -> &adw::TimedAnimation {
        self.imp().countdown_ani.get().unwrap()
    }

    fn draw_border(&self, snapshot: &gtk::Snapshot, size: f32, border_width: f32) {
        // Magic matrix that turns the inner blue circle transparent.
        #[rustfmt::skip]
        let color_matrix = graphene::Matrix::from_float([
            1.0, -1.0, -1.0,  0.0,
            1.0,  1.0,  1.0,  1.0,
            1.0,  1.0,  1.0, -1.0,
            0.0,  0.0,  0.0,  1.0,
        ]);
        let color_offset = graphene::Vec4::from_float([0.0; 4]);
        snapshot.push_color_matrix(&color_matrix, &color_offset);

        let countdown = self.imp().countdown_val.get() as f32;

        let rect = graphene::Rect::new(0.0, 0.0, size, size);
        let center = graphene::Point::new(size / 2.0, size / 2.0);

        let s = graphene::Size::new(size / 2.0, size / 2.0);
        let rounded = gsk::RoundedRect::new(rect, s, s, s, s);

        let color = self.color();

        let stop0 = gsk::ColorStop::new(countdown, color);
        let stop1 = gsk::ColorStop::new(countdown, gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));

        snapshot.push_rounded_clip(&rounded);
        snapshot.append_conic_gradient(&rect, &center, 0.0, &[stop0, stop1]);
        snapshot.pop();

        // We draw a blue circle, later the color matrix will turn blue into
        // transparent goodness.
        let inner_size = size - 2.0 * border_width;
        let inner_rect = graphene::Rect::new(border_width, border_width, inner_size, inner_size);
        let inner_s = graphene::Size::new(inner_size / 2.0, inner_size / 2.0);
        let inner_rounded = gsk::RoundedRect::new(inner_rect, inner_s, inner_s, inner_s, inner_s);

        snapshot.push_rounded_clip(&inner_rounded);
        // We color on a bigger rect than what we clipped.
        snapshot.append_color(&gdk::RGBA::BLUE, &rect);
        snapshot.pop();

        snapshot.pop(); // Pop the color matrix.
    }

    fn draw_play(&self, snapshot: &gtk::Snapshot, size: f32, border_width: f32) {
        let imp = self.imp();

        // When we want to transform the button to a square, we don't use the
        // gap animation, otherwise it looks janky as its size gets smaller due
        // to the record animation and its size at first is momentarily reduced
        // due to the press animation, but later it gets bigger.
        let gap = if matches!(imp.shutter_mode.get(), crate::ShutterMode::Picture)
            || self.countdown() > 0
        {
            imp.press_val.get() as f32
        } else {
            4.0
        };
        let record = imp.record_val.get() as f32;

        let initial_r_size = size - 2.0 * gap - 2.0 * border_width;
        let x = gap + border_width + initial_r_size * LAMBDA * record;
        let y = x;
        let rect_size = size - 2.0 * x;
        let rect = graphene::Rect::new(x, y, rect_size, rect_size);
        let big_rect = graphene::Rect::new(0.0, 0.0, size, size);

        let border_radius = (1.0 - record * 0.7) * rect_size / 2.0;
        let s = graphene::Size::new(border_radius, border_radius);
        let rounded = gsk::RoundedRect::new(rect, s, s, s, s);

        let from_color = self.color();
        // This is Red 3 from the palette.
        let to_color = gdk::RGBA::new(0.8784314, 0.14117648, 0.105882354, 1.0);
        let t = imp.mode_val.get() as f32;
        let color = color_lerp(t, from_color, to_color);
        snapshot.push_rounded_clip(&rounded);
        // We color on a bigger rect than what we clipped.
        snapshot.append_color(&color, &big_rect);
        snapshot.pop();
    }
}

#[inline]
fn lerp(t: f32, from: f32, to: f32) -> f32 {
    to * t + (1.0 - t) * from
}

/// Does interpolation between two colors, preserves the alpha of the original.
fn color_lerp(t: f32, from: gdk::RGBA, to: gdk::RGBA) -> gdk::RGBA {
    gdk::RGBA::new(
        lerp(t, from.red(), to.red()),
        lerp(t, from.green(), to.blue()),
        lerp(t, from.blue(), to.blue()),
        from.alpha(),
    )
}
