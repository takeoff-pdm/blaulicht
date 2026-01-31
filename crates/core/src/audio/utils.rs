///
/// System messages.
///
#[macro_export]
macro_rules! system_message {
    ($now:ident,$last_publish:ident,$system_out:ident,$tx_signal:expr) => {
        if $now - $last_publish > SYSTEM_MESSAGE_SPEED.as_millis() as u64 {
            for signal in $tx_signal {
                $system_out.send(signal.clone()).unwrap();
            }
            $last_publish = $now
        }
    };
}

///
/// Vector push operations.
///
#[macro_export]
macro_rules! shift_push {
    ($vector:expr,$capacity:ident,$item:expr) => {
        $vector.push_back($item);
        if $vector.len() > $capacity {
            $vector.pop_front();
        }
    };
}
