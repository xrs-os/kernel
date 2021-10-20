#![no_std]

#[macro_export]
macro_rules! num_enum {
    ($v:vis $name: ident: u8 { $( $item_name:ident = $item_value:literal),+,} ) => {

        #[repr(u8)]
        #[derive(Eq, PartialEq, Debug, Copy, Clone, Ord, PartialOrd)]
        $v enum $name {
            $($item_name = $item_value),+
        }
        num_enum::num_enum!(__inner $v $name: u8 {$( $item_name = $item_value),+});
    };
    ($v:vis $name: ident: u16 { $( $item_name:ident = $item_value:literal),+,} ) => {

        #[repr(u16)]
        #[derive(Eq, PartialEq, Debug, Copy, Clone, Ord, PartialOrd)]
        $v enum $name {
            $($item_name = $item_value),+
        }
        num_enum::num_enum!(__inner $v $name: u16 {$( $item_name = $item_value),+});
    };
    ($v:vis $name: ident: u32 { $( $item_name:ident = $item_value:literal),+,} ) => {

        #[repr(u32)]
        #[derive(Eq, PartialEq, Debug, Copy, Clone, Ord, PartialOrd)]
        $v enum $name {
            $($item_name = $item_value),+
        }
        num_enum::num_enum!(__inner $v $name: u32 {$( $item_name = $item_value),+});
    };
    ($v:vis $name: ident: u64 { $( $item_name:ident = $item_value:literal),+,} ) => {

        #[repr(u64)]
        #[derive(Eq, PartialEq, Debug, Copy, Clone, Ord, PartialOrd)]
        $v enum $name {
            $($item_name = $item_value),+
        }
        num_enum::num_enum!(__inner $v $name: u64 {$( $item_name = $item_value),+});
    };
    (__inner $v:vis $name: ident : $repr:ty { $( $item_name:ident = $item_value:literal),+} ) => {

        impl $name {
            pub const fn from_primitive(item: $repr) -> Option<Self> {
                match item {
                    $($item_value => Some($name::$item_name)),+,

                    _ => None
                }
            }

            pub const fn to_primitive(self) -> $repr {
                self as $repr
            }
        }

        impl From<$name> for $repr {
            fn from(item: $name) -> Self {
                item as $repr
            }
        }

    };
}
