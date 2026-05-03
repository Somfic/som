#[macro_export]
macro_rules! expand_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(
                $variant:ident $({ $($field:ident : $field_ty:ty),* $(,)? })?
            ),* $(,)?
        } with {
            $($extra_field:ident : $extra_ty:ty),* $(,)?
        }
    ) => {
        $crate::__expand_enum_munch! {
            attrs($(#[$meta])*)
            name($name)
            extras($($extra_field: $extra_ty),*)
            variants( $( ($variant $({ $($field: $field_ty),* })?) )* )
            done()
        }

        $crate::__expand_enum_accessors! {
            name($name)
            variants($($variant)*)
            extras_todo($($extra_field: $extra_ty),*)
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __expand_enum_munch {
    // base case: no more variants, emit the enum.
    (
        attrs($($attrs:tt)*)
        name($name:ident)
        extras($($_e:tt)*)
        variants()
        done($($body:tt)*)
    ) => {
        $($attrs)*
        pub enum $name {
            $($body)*
        }
    };

    // step: pop one variant, splice extras + its fields into its body.
    (
        attrs($($attrs:tt)*)
        name($name:ident)
        extras($($extra_field:ident : $extra_ty:ty),*)
        variants(
            ($variant:ident $({ $($field:ident : $field_ty:ty),* })?)
            $($rest:tt)*
        )
        done($($body:tt)*)
    ) => {
        $crate::__expand_enum_munch! {
            attrs($($attrs)*)
            name($name)
            extras($($extra_field: $extra_ty),*)
            variants($($rest)*)
            done(
                $($body)*
                $variant {
                    $( $extra_field: $extra_ty, )*
                    $($( $field: $field_ty, )*)?
                },
            )
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __expand_enum_accessors {
    // base case: emit the (possibly empty) impl block.
    (
        name($name:ident)
        variants($($variant:ident)*)
        extras_todo()
        $(fns($($fns:tt)*))?
    ) => {
        impl $name {
            $($($fns)*)?
        }
    };

    // step: pop one extra, emit `fn extra(&self) -> ExtraTy`.
    (
        name($name:ident)
        variants($($variant:ident)*)
        extras_todo($extra_field:ident : $extra_ty:ty $(, $rest_field:ident : $rest_ty:ty)* $(,)?)
        $(fns($($fns:tt)*))?
    ) => {
        $crate::__expand_enum_accessors! {
            name($name)
            variants($($variant)*)
            extras_todo($($rest_field : $rest_ty),*)
            fns(
                $($($fns)*)?
                pub fn $extra_field(&self) -> $extra_ty
                where $extra_ty: Clone
                {
                    match self {
                        $( $name::$variant { $extra_field, .. } => $extra_field.clone(), )*
                    }
                }
            )
        }
    };
}
