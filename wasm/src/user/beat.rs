//
// Macros.
//

#[macro_export]
macro_rules! if_beat {
    ($input:expr, $state:expr, $body: expr, $not_body: expr) => {
        let mut time_between_beats_millis = $input.time_between_beats_millis as u32;
        if !$state.animation.strobe.controls.on_beat {
            time_between_beats_millis = 200;
        }

        if (time_between_beats_millis > 0
            && $input.time - $state.last_beat_time >= time_between_beats_millis)
        {
            // Check that there is a beat.
            if ($input.bass_avg > 70 || $input.bass > 100)
                || !$state.animation.strobe.controls.on_beat
            {
                $state.last_beat_time = $input.time;
                $body;
            } else {
                $not_body;
            }
        } else {
            $not_body;
        }
    };
}