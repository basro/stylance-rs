pub use macros::*;

#[cfg(feature = "nightly")]
#[macro_export]
macro_rules! import_style {
    ($ident:ident, $str:expr) => {
        mod $ident {
            ::stylance::import_style_classes_rel!($str);
        }
    };
    (pub $ident:ident, $str:expr) => {
        pub mod $ident {
            ::stylance::import_style_classes_rel!($str);
        }
    };
}

#[macro_export]
macro_rules! import_crate_style {
    ($ident:ident, $str:expr) => {
        mod $ident {
            ::stylance::import_style_classes!($str);
        }
    };
    (pub $ident:ident, $str:expr) => {
        pub mod $ident {
            ::stylance::import_style_classes!($str);
        }
    };
}
