use futures_util::stream::StreamExt;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::iterator::Handle;
use signal_hook_tokio::Signals;
use std::future::Future;
use std::io;
use std::pin::Pin;

pub fn get_signals_fut() -> io::Result<(Pin<Box<impl Future<Output = Option<i32>> + Sized>>, Handle)>
{
    let signals = Signals::new(TERM_SIGNALS)?;
    let handle = signals.handle();
    Ok((
        Box::pin(async move {
            let mut signals_stream = signals.fuse();
            signals_stream.next().await
        }),
        handle,
    ))
}
