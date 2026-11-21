//! A round of questions held at a terminal until every one is answered, and what stays
//! on the screen after it: each question with its answer, whole.

use surco::surface::said_whole;
use yunta_core::diagnostic::Report;
use yunta_core::events::Channel;
use yunta_core::port::QuestionsReply;
use yunta_core::{Env, NodeId, QuestionsFile};
use yunta_render::prompt::FOLDED;
use yunta_ui::prompt::{Round, SettledAnswers};

use super::held::hold;
use super::{attributed, Answered, Console};

/// Puts every question `node` asks in `questions` to the person as one round, what the
/// engine `refused` of the answers it was last given standing under each question it is
/// about, and records the answers once the last of them is given.
pub(crate) fn answer(
    console: &Console,
    env: &Env,
    (node, questions): (&NodeId, &QuestionsFile),
    refused: Option<&Report>,
) -> Answered<QuestionsReply> {
    let settled = match questions.questions.is_empty() {
        true => SettledAnswers::default(),
        false => {
            let mut round = Round::new(node, questions, console.escape()).refused_as(refused);
            hold(console, &mut round)?
        }
    };
    let look = console.look();
    for (asked, said) in &settled.said {
        for row in said_whole(asked, said, &look) {
            console.say(&look.paint.line(&row))?;
        }
    }
    if settled.folded {
        console.say(FOLDED)?;
    }
    Ok(QuestionsReply {
        answers: settled.answers,
        channel: Channel::Tty,
        responder: Some(attributed(console, env)?),
    })
}
