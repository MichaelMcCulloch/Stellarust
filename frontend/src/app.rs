use yew::{html, Component};

pub struct App {}
pub enum Msg {}

impl Component for App {
    type Message = Msg;

    type Properties = ();

    fn create(ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        html! {}
    }
    fn changed(&mut self, ctx: &yew::Context<Self>) -> bool {
        todo!()
    }
    fn update(&mut self, ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        todo!()
    }
}
