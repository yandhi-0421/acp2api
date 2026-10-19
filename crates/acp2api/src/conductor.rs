use crate::{
    Clock, Hold,
    conversation::{Answer, TurnRequest},
    runtime_driver::Runtime,
};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
#[derive(Clone)]
pub(crate) struct Conductor(mpsc::Sender<Work>);
struct Work {
    turn: TurnRequest,
    hold: Hold,
    answer: oneshot::Sender<Result<Answer, String>>,
}
struct Core;
impl Core {
    fn prompt(&mut self, turn: &TurnRequest) -> String {
        crate::conversation::prompt(turn)
    }
}
impl Conductor {
    pub(crate) fn start(
        mut runtime: Runtime,
        clock: Arc<dyn Clock>,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (send, mut recv) = mpsc::channel::<Work>(32);
        let task = tokio::spawn(async move {
            let mut core = Core;
            loop {
                let timer = clock.sleep_until(u64::MAX);
                tokio::select! {
                    work = recv.recv() => { let Some(work) = work else { break }; let prompt = core.prompt(&work.turn); drop(work.hold); let answer = runtime.prompt(prompt, &work.turn.model).await.map(|(text, input_tokens)| { let output_tokens = crate::conversation::output_tokens(&text); Answer { text, input_tokens, output_tokens } }); let _ = work.answer.send(answer); },
                    work = timer => { drop(work); }
                }
            }
            runtime.stop().await;
        });
        (Self(send), task)
    }
    pub(crate) async fn answer(&self, turn: TurnRequest, hold: Hold) -> Result<Answer, String> {
        let (answer, read) = oneshot::channel();
        self.0
            .send(Work { turn, hold, answer })
            .await
            .map_err(|_| "conductor stopped")?;
        read.await.map_err(|_| "conductor stopped")?
    }
}
