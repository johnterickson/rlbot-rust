use crate::{ffi, game, rlbot::RLBot};
use std::{
    error::Error,
    mem::MaybeUninit,
    time::{Duration, Instant},
};

/// An iterator-like object that yields physics ticks from the game as they
/// occur.
pub struct Physicist<'a> {
    rlbot: &'a RLBot,
    ratelimiter: ratelimit::Limiter,
    prev_ball_frame: i32,
}

impl<'a> Physicist<'a> {
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

    pub(crate) fn new(rlbot: &'a RLBot) -> Self {
        // Physics ticks happen at 120Hz. The goal is never to miss any. But if we poll
        // too often, the game crashes, so space out the checks.
        let ratelimiter = ratelimit::Builder::new()
            .interval(Duration::from_millis(1))
            .build();

        Self {
            rlbot,
            ratelimiter,
            prev_ball_frame: 0,
        }
    }

    /// Block until the next physics tick occurs, and then return it.
    ///
    /// # Errors
    ///
    /// This function returns an error if ten seconds pass without a new tick
    /// being received. The assumption is that the game froze or crashed, and
    /// waiting longer will not help.
    #[allow(clippy::should_implement_trait)]
    #[deprecated(
        note = "the struct-based methods are deprecated; use the flatbuffer equivalents instead"
    )]
    #[allow(deprecated)]
    pub fn next(&mut self) -> Result<ffi::RigidBodyTick, Box<dyn Error>> {
        self.spin(|this| Ok(this.try_next()?), Self::DEFAULT_TIMEOUT)
    }

    /// Polls for a new physics tick.
    ///
    /// If there is a tick that is newer than the previous tick, it is
    /// returned. Otherwise, `None` is returned.
    #[deprecated(
        note = "the struct-based methods are deprecated; use the flatbuffer equivalents instead"
    )]
    #[allow(deprecated)]
    pub fn try_next(&mut self) -> Result<Option<ffi::RigidBodyTick>, Box<dyn Error>> {
        let mut result = MaybeUninit::zeroed();
        self.rlbot.interface().update_rigid_body_tick(result.as_mut_ptr())?;
        let result = unsafe { result.assume_init() };
        if result.Ball.State.Frame != self.prev_ball_frame {
            self.prev_ball_frame = result.Ball.State.Frame;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    /// Block until the next physics tick occurs, and then return it.
    ///
    /// # Errors
    ///
    /// This function returns an error if ten seconds pass without a new tick
    /// being received. The assumption is that the game froze or crashed, and
    /// waiting longer will not help.
    pub fn next_flat(&mut self) -> Result<game::RigidBodyTick, Box<dyn Error>> {
        self.spin(|this| Ok(this.try_next_flat()), Self::DEFAULT_TIMEOUT)
    }

    /// Block until the next physics tick occurs, and then return it.
    ///
    /// This works the same as `next_flat`, but lets the caller choose the
    /// timeout.
    pub fn next_flat_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<game::RigidBodyTick, Box<dyn Error>> {
        self.spin(|this| Ok(this.try_next_flat()), timeout)
    }

    /// Polls for a new physics tick.
    ///
    /// If there is a tick that is newer than the previous tick, it is
    /// returned. Otherwise, `None` is returned.
    #[allow(clippy::redundant_closure)]
    pub fn try_next_flat(&mut self) -> Option<game::RigidBodyTick> {
        if let Some(tick) = self.rlbot.interface().update_rigid_body_tick_flatbuffer() {
            let ball = &tick.ball;
            match ball
                .as_ref()
                .and_then(|b| b.state.as_ref())
                .map(|s| s.frame)
            {
                Some(ball_frame) if ball_frame != self.prev_ball_frame => {
                    self.prev_ball_frame = ball_frame;
                    return Some(tick);
                }
                _ => {}
            }
        }
        None
    }

    /// Keep trying `f` until the timeout elapses.
    fn spin<R>(
        &mut self,
        f: impl Fn(&mut Self) -> Result<Option<R>, Box<dyn Error>>,
        timeout: Duration,
    ) -> Result<R, Box<dyn Error>> {
        let start = Instant::now();

        loop {
            self.ratelimiter.wait();

            if let Some(tick) = f(self)? {
                return Ok(tick);
            }

            let elapsed = Instant::now() - start;
            if elapsed > timeout {
                return Err(From::from("no physics tick received within the timeout"));
            }
        }
    }
}
