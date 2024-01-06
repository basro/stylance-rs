pub use macros::import_style_classes;

#[macro_export]
macro_rules! import_style {
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
