//! A single-child container that reports its allocated size, used to scale
//! fonts with the widget. Event-driven: no timers, no tick callbacks.

use std::cell::RefCell;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ResizeBin {
        pub child: RefCell<Option<gtk::Widget>>,
        pub callback: RefCell<Option<Box<dyn Fn(i32, i32)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResizeBin {
        const NAME: &'static str = "SamCalcResizeBin";
        type Type = super::ResizeBin;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("resizebin");
        }
    }

    impl ObjectImpl for ResizeBin {
        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ResizeBin {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.child.borrow().as_ref() {
                Some(child) => child.measure(orientation, for_size),
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.child.borrow().as_ref() {
                child.allocate(width, height, baseline, None);
            }
            if let Some(cb) = self.callback.borrow().as_ref() {
                cb(width, height);
            }
        }
    }
}

glib::wrapper! {
    pub struct ResizeBin(ObjectSubclass<imp::ResizeBin>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ResizeBin {
    pub fn new(child: &impl IsA<gtk::Widget>) -> Self {
        let bin: Self = glib::Object::new();
        child.set_parent(&bin);
        *bin.imp().child.borrow_mut() = Some(child.clone().upcast());
        bin
    }

    pub fn connect_resized(&self, f: impl Fn(i32, i32) + 'static) {
        *self.imp().callback.borrow_mut() = Some(Box::new(f));
    }
}
