//! See [`super`]. What the note says of a prompt in place let go as the run stands at its
//! pause no longer, by the answer the run's log holds to that pause: answered elsewhere, who
//! answered and how, in the log's words, so what the person typed or picked on it goes with
//! a reason they read; answered here, what the run does now, where the log came before the
//! answer's own outcome; nothing where the log holds no answer to it (a run stopped at that
//! pause, or one the engine took past it by itself), or where the answer given here was
//! noted already.
//!
//! The answer to a decision is the first resolution recorded against what it is on, its
//! node or the run, after the pause it belongs to. The answer to a round is the one its
//! `questions_asked` got, while no pause came after it: a pause after an answer is the engine
//! refusing it, or the run stopped before it acted on it.

use yunta_api::FollowedRun;
use yunta_replay::RunState;

use super::answer::Answer;
use super::{InPlace, Notice, Posed};

impl InPlace {
    /// What the note says as it is let go, `run` standing as the following last read it: who
    /// answered its pause elsewhere, in the log's words; what the run does now, where the
    /// answer under way here is the one the log holds; none where the log holds no answer to
    /// its pause, the answer given here was noted already, or `run` is not the run its pause
    /// is of, as when the stage moved onto a child, a parent or a successor.
    pub(super) fn noted_as_it_goes(&self, run: &FollowedRun) -> Option<Notice> {
        if self.pause.run() != run.run() {
            return None;
        }
        let answered_elsewhere = self.answered_as_logged(run.state())?;
        match &self.answer {
            Answer::Open => Some(Notice::AnsweredElsewhere(answered_elsewhere)),
            Answer::UnderWay(said) => Some(Notice::Recorded(said.clone())),
            Answer::Recorded => None,
        }
    }

    /// The answer the run's log `state` holds to the pause it belongs to, as render notes an
    /// answer given elsewhere: none where it holds none.
    fn answered_as_logged(&self, state: &RunState) -> Option<String> {
        let paused_at = self.pause.seq();
        let gates = &state.gates;
        match &self.posed {
            Posed::Decision(deciding) => {
                let resolved = match &deciding.decision().node {
                    Some(node) => gates
                        .get(node)?
                        .resolved
                        .iter()
                        .find(|(_, at)| *at > paused_at)
                        .map(|(resolved, _)| resolved),
                    None => gates
                        .run_answer()
                        .filter(|(_, at)| *at > paused_at)
                        .map(|(resolved, _)| resolved),
                }?;
                Some(yunta_render::screen::answered_elsewhere(resolved))
            }
            Posed::Round { node, .. } => {
                let rounds = &gates.get(node)?.rounds;
                let round = rounds.iter().find(|round| round.asked_at == paused_at)?;
                let answered_at = round.answered_at?;
                let paused_since = state.run.last_pause();
                let refused_or_stopped = paused_since.is_some_and(|(_, at)| at > answered_at);
                let answered = round.answered.as_ref().filter(|_| !refused_or_stopped)?;
                Some(yunta_render::screen::round_answered_elsewhere(answered))
            }
        }
    }
}
