use core::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};
use paste::paste;
use pin_project::pin_project;

macro_rules! with_arg {


    (fn $func_name: ident ( $( $arg_name:ident : $arg_type:ident),+) -> $type_name:ident ) => {
        paste!{
            impl<T: ?Sized> [<$type_name Ext>] for T where T: Future {}

            pub trait [<$type_name Ext>]: Future {
                fn $func_name<$($arg_type),+>(self, $($arg_name:$arg_type),+) -> $type_name<Self, $($arg_type),+>
                where
                    Self: Sized,
                {
                    $type_name::new(self,$($arg_name),+)
                }

            }

            #[pin_project(project=[<$type_name Proj>], project_replace=[<$type_name ProjReplace>])]
            pub enum $type_name<Fut, $($arg_type),+> {
                Incomplete {
                    #[pin]
                    future: Fut,
                    $(
                        $arg_name:$arg_type
                    ),+
                },
                Complete,
            }

            impl<Fut, $($arg_type),+> $type_name<Fut, $($arg_type),+> {
                pub(crate) fn new(future: Fut, $($arg_name:$arg_type),+) -> Self {
                    Self::Incomplete { future, $($arg_name),+ }
                }
            }

            impl<Fut, T, $($arg_type),+> Future for $type_name<Fut, $($arg_type),+>
            where
                Fut: Future<Output = T>,
            {
                type Output = (T, $($arg_type),+);

                fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                    match self.as_mut().project() {
                        [<$type_name Proj>]::Incomplete { future, .. } => {
                            let output = ready!(future.poll(cx));
                            match self.project_replace($type_name::Complete) {
                                [<$type_name ProjReplace>]::Incomplete { $($arg_name),+, .. } => Poll::Ready((output, $($arg_name),+)),
                                [<$type_name ProjReplace>]::Complete => unreachable!(),
                            }
                        }
                        [<$type_name Proj>]::Complete => {
                            panic!(concat!(stringify!($type_name), " must not be polled after it returned `Poll::Ready`"))
                        }
                    }
                }
            }
        }

    }
}

with_arg!(fn with_arg1(a: A) -> WithArg1);
with_arg!(fn with_arg2(a: A, b: B) -> WithArg2);
with_arg!(fn with_arg3(a: A, b: B, c: C) -> WithArg3);
with_arg!(fn with_arg4(a: A, b: B, c: C, d: D) -> WithArg4);
with_arg!(fn with_arg5(a: A, b: B, c: C, d: D, e: E) -> WithArg5);
with_arg!(fn with_arg6(a: A, b: B, c: C, d: D, e: E, f: F) -> WithArg6);
