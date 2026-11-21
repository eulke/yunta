//! See [`super`]. This moment as a screen: the stage's, or what stands before the
//! application knows where it stands; the window's title; and what stands over the stage,
//! a confirmation, the palette or help, each in its box, help listing the keys the stage
//! under it says. The palette's line is typed, so the stage under it is drawn for a line
//! typed there. A stage that draws no far keys of its own
//! takes the keys every stage answers to after its own, where nothing stands over it and no
//! line is typed.

use surco::components::{Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;

use super::State;
use crate::dialog::Dialog;
use crate::Stage;

impl State {
    /// This moment, as a screen any surface lays out.
    pub fn screen(&self, look: &Look) -> Screen {
        let typed = matches!(self.dialog, Some(Dialog::Palette(_)));
        let mut screen = match &self.stage {
            Some(stage) => stage.screen(look, &self.followed_runs, self.now, typed),
            None => self.unopened_screen(),
        };
        screen.title = self.calling.title(&self.followed_runs, look.glyphs);
        let typing = |stage: &Stage| stage.typing(&self.followed_runs, self.now);
        let keyed = self.stage.as_ref().is_some_and(|stage| !typing(stage));
        match &self.dialog {
            Some(Dialog::Confirmation(confirmation)) => {
                screen.overlay = Some(confirmation.overlay());
            }
            Some(Dialog::Palette(palette)) => screen.overlay = Some(palette.overlay()),
            Some(Dialog::Help(help)) => {
                let listed = match &self.stage {
                    Some(stage) => stage.help(&screen.legend, &self.followed_runs, self.now),
                    None => yunta_render::screen::help(&screen.legend),
                };
                screen.overlay = Some(help.overlay(listed));
            }
            None if keyed && screen.anywhere.is_empty() => {
                let legend = std::mem::take(&mut screen.legend);
                screen.legend = yunta_render::screen::over_every_stage(legend);
            }
            None => {}
        }
        screen
    }

    /// The screen before the application knows where it stands: nothing; the runs
    /// being read once it knows the repository; or what stopped it learning, with
    /// leaving left.
    fn unopened_screen(&self) -> Screen {
        let doc = match (&self.unopened, self.repository) {
            (Some(said), _) => Doc::new().with(Note(said.clone())),
            (None, Some(_)) => {
                Doc::new().with(Note(yunta_render::screen::READING_THE_RUNS.to_string()))
            }
            (None, None) => Doc::new(),
        };
        let mut screen = Screen::new(Region::new(doc));
        screen.trail = vec![yunta_render::screen::YUNTA.to_string()];
        if self.unopened.is_some() {
            screen.legend = yunta_render::screen::leaving_legend();
        }
        screen
    }
}
