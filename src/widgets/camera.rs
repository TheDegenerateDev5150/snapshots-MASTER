// SPDX-License-Identifier: GPL-3.0-or-later
use std::os::unix::prelude::RawFd;

use ashpd::desktop::camera;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use gtk::{prelude::*, CompositeTemplate};

use crate::{CameraRow, Device};

mod imp {
    use super::*;

    use std::cell::RefCell;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/World/Snapshot/ui/camera.ui")]
    pub struct Camera {
        pub paintable: crate::CameraPaintable,
        pub stream_list: RefCell<gio::ListStore>,
        pub selection: gtk::SingleSelection,
        pub provider: RefCell<Option<crate::DeviceProvider>>,

        #[template_child]
        pub gallery_button: TemplateChild<crate::GalleryButton>,
        #[template_child]
        pub camera_menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub picture: TemplateChild<gtk::Picture>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub shutter_button: TemplateChild<crate::ShutterButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Camera {
        const NAME: &'static str = "Camera";
        type Type = super::Camera;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Camera {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.paintable.set_picture(&*self.picture);

            let popover = gtk::Popover::new();
            popover.add_css_class("menu");

            let stream_list = gio::ListStore::new(glib::BoxedAnyObject::static_type());
            self.stream_list.replace(stream_list.clone());
            self.selection.set_model(Some(&stream_list));
            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(|_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                let camera_row = CameraRow::default();

                item.set_child(Some(&camera_row));
            });
            let selection = &self.selection;
            factory.connect_bind(glib::clone!(@weak selection => move |_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                let child = item.child().unwrap();
                let row = child.downcast_ref::<CameraRow>().unwrap();

                let item = item.item().unwrap().downcast::<glib::BoxedAnyObject>().unwrap();
                let camera_item = item.borrow::<Device>();
                row.set_item(&camera_item);

                selection.connect_selected_item_notify(glib::clone!(@weak row, @weak item => move |selection| {
                    if let Some(selected_item) = selection.selected_item() {
                        row.set_selected(selected_item == item);
                    } else {
                        row.set_selected(false);
                    }
                }));
            }));
            let list_view = gtk::ListView::new(Some(&self.selection), Some(&factory));

            popover.set_child(Some(&list_view));

            self.selection.connect_selected_item_notify(glib::clone!(@weak obj, @weak popover => move |selection| {
                if let Some(selected_item) = selection.selected_item() {
                    let device = selected_item.downcast_ref::<glib::BoxedAnyObject>().unwrap().borrow::<Device>();
                    obj.imp().paintable.set_pipewire_element(device.element.clone());
                }
                popover.popdown();
            }));

            self.camera_menu_button.set_popover(Some(&popover));

            self.paintable.connect_code_detected(|_, code| {
                // TODO Do a proper dialog here.
                log::debug!("Found QR code with contents: {code}");
            })
        }

        fn dispose(&self) {
            self.obj().dispose_template(Self::Type::static_type());
        }
    }
    impl WidgetImpl for Camera {}
}

glib::wrapper! {
    pub struct Camera(ObjectSubclass<imp::Camera>)
        @extends gtk::Widget;
}

impl Default for Camera {
    fn default() -> Self {
        glib::Object::new(&[])
    }
}

impl Camera {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.imp().paintable.close_pipeline();
    }

    pub fn start(&self) {
        let imp = self.imp();
        imp.spinner.start();
        imp.stack.set_visible_child_name("loading");

        let ctx = glib::MainContext::default();
        ctx.spawn_local(glib::clone!(@weak self as camera => async move {
            let imp = camera.imp();

            match camera.try_start_stream().await {
                Ok(()) => imp.stack.set_visible_child_name("camera"),
                Err(err) => {
                    imp.stack.set_visible_child_name("not-found");
                    log::debug!("Could not find camera: {}", err);
                }
            };
            imp.spinner.stop();
        }));
    }

    async fn try_start_stream(&self) -> anyhow::Result<()> {
        let imp = self.imp();
        // let stream_fd = match stream().await {
        //     Ok(stream_fd) => Some(stream_fd),
        //     Err(err) => {
        //         log::debug!("Failed to communicate with the Camera portal: {err}");
        //         None
        //     }
        // };

        // TODO We pass None since the portal does not return microphones and
        // creating additional DeviceProviders does not work.
        let provider = crate::DeviceProvider::new(None)?;

        // TODO Improve this, we just try with the first mic we find. One could try
        // matching pairs of AudioSrc and AudioSink.
        if let Some(mic) = provider.mics().first() {
            // FIXME This crashes/freezes the app
            self.imp().paintable.set_pipewire_mic(mic.element.clone());
        }

        for item in provider.cameras() {
            imp.stream_list
                .borrow()
                .append(&glib::BoxedAnyObject::new(item));
        }
        imp.selection.set_selected(0);

        imp.provider.replace(Some(provider));

        Ok(())
    }

    pub async fn start_recording(&self, format: crate::VideoFormat) -> anyhow::Result<()> {
        self.imp().paintable.start_recording(format)?;
        Ok(())
    }

    pub async fn stop_recording(&self) -> anyhow::Result<()> {
        self.imp().paintable.stop_recording();
        Ok(())
    }

    pub async fn take_picture(&self, format: crate::PictureFormat) -> anyhow::Result<()> {
        self.imp().paintable.take_snapshot(format)
    }

    pub fn set_countdown(&self, countdown: u32) {
        self.imp().shutter_button.set_countdown(countdown);
    }

    pub fn start_countdown(&self) {
        self.imp().shutter_button.start_countdown();
    }

    pub fn stop_countdown(&self) {
        self.imp().shutter_button.stop_countdown();
    }

    pub fn shutter_mode(&self) -> crate::ShutterMode {
        self.imp().shutter_button.shutter_mode()
    }

    pub fn set_shutter_mode(&self, shutter_mode: crate::ShutterMode) {
        self.imp().shutter_button.set_shutter_mode(shutter_mode);
    }

    pub fn set_gallery(&self, gallery: crate::Gallery) {
        let imp = self.imp();

        imp.paintable
            .connect_picture_stored(glib::clone!(@weak gallery,  => move |_, texture| {
                gallery.add_image(texture);
            }));
        imp.gallery_button.set_gallery(gallery);
    }
}

async fn stream() -> ashpd::Result<RawFd> {
    let proxy = camera::Camera::new().await?;
    proxy.request_access().await?;

    proxy.open_pipe_wire_remote().await
}
