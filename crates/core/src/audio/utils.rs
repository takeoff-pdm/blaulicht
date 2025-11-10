#[macro_export]
macro_rules! system_message {
    ($now:ident,$last_publish:ident,$system_out:ident,$tx_signal:expr) => {
        if $now - $last_publish > SYSTEM_MESSAGE_SPEED {
            for signal in $tx_signal {
                $system_out.send(signal.clone()).unwrap();
            }
            $last_publish = $now
        }
    };
}

#[macro_export]
macro_rules! signal {
    ($sink:ident,$tx_signal:expr) => {
        let signal_res = $tx_signal;

        // for signal in signal_res {
        //     if let Err(e) = $out0.send(signal.clone()) {
        //         log::warn!("[AUDIO] Shutting down thread.");
        //         anyhow::bail!(e.to_string());
        //         // return Err(e.to_string());
        //     }
        // }

        for signal in signal_res {
            $sink.signal(signal.clone());
        }
    };
}

///
///
/// Vector push operations.
///
///

#[macro_export]
macro_rules! shift_push {
    ($vector:ident,$capacity:ident,$item:expr) => {
        $vector.push_back($item);
        if $vector.len() > $capacity {
            $vector.pop_front();
        }
    };
}
