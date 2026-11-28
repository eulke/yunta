//! See [`super`]. This moment as a screen: the stage's, or what stands before the
//! application knows where it stands; the window's title; and what stands over the stage,
//! a confirmation, the palette or help, each in its box, help listing the keys the stage
//! under it takes, where `q` goes down to among them. The palette's line is typed, so the
//! stage under it is drawn for a line typed there.

use surco::components::{Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_render::screen::GoingDown;

use super::State;
use crate::dialog::Dialog;
use crate::Stage;

impl State {
    /// This moment, as a screen any surface lays out.
    pub fn screen(&self, look: &Look) -> Screen {
        let typed = matches!(self.dialog, Some(Dialog::Palette(_)));
        let mut screen = match self.stage() {
            Some(stage) => stage.screen(look, &self.followed_runs, self.now, typed),
            None => self.unopened_screen(),
        };
        screen.title = self.calling.title(&self.followed_runs, look.glyphs);
        match &self.dialog {
            Some(Dialog::Confirmation(confirmation)) => {
                screen.overlay = Some(confirmation.overlay());
            }
            Some(Dialog::Palette(palette)) => screen.overlay = Some(palette.overlay()),
            Some(Dialog::Help(help)) => {
                let (followed, now) = (&self.followed_runs, self.now);
                let down = self.going_down();
                let listed = self
                    .stage()
                    .map(|stage| stage.help(&screen.legend, followed, now, down));
                if let Some(listed) = listed {
                    screen.overlay = Some(help.overlay(listed));
                }
            }
            None => {}
        }
        screen
    }

    /// Where `q` goes down to from a page, as help says it: the nearest field under the stage
    /// the person is at, or, where none stands under it, the stage at the bottom, where the
    /// application opened.
    fn going_down(&self) -> GoingDown {
        match self
            .stages_under()
            .any(|stage| matches!(stage, Stage::Field(_)))
        {
            true => GoingDown::ToTheField,
            false => GoingDown::ToWhereItOpened,
        }
    }

    /// The screen before the application knows where it stands: nothing; the runs being
    /// read once it knows the repository, and under them a look at them that failed, as the
    /// field says one under its list; or what stopped it learning. Where something stopped it
    /// or a look failed, leaving is left.
    fn unopened_screen(&self) -> Screen {
        let failed = self.followed_runs.look_failed.as_ref();
        let doc = match (&self.unopened, &self.followed_runs.repository) {
            (Some(said), _) => Doc::new().with(Note(said.clone())),
            (None, Some(_)) => {
                let reading = yunta_render::screen::READING_THE_RUNS.to_string();
                let doc = Doc::new().with(Note(reading));
                match failed {
                    Some(failed) => doc.with(Note(failed.clone())),
                    None => doc,
                }
            }
            (None, None) => Doc::new(),
        };
        let mut screen = Screen::new(Region::new(doc));
        screen.trail = vec![yunta_render::screen::YUNTA.to_string()];
        if self.unopened.is_some() || failed.is_some() {
            screen.legend = yunta_render::screen::leaving_legend();
        }
        screen
    }
}
