use yew::prelude::*;
use plotters::prelude::*;
use plotters::drawing::IntoDrawingArea;

#[function_component(Graph)]
pub(crate) fn graph() -> Html {

    let mut buff = String::new();
    {
        let root_drawing_area = SVGBackend::with_string(&mut buff, (256,256))
        .into_drawing_area();

        root_drawing_area.fill(&WHITE).unwrap();

        let mut chart = ChartBuilder::on(&root_drawing_area)
            .build_cartesian_2d(-3.14..3.14, -1.2..1.2)
            .unwrap();

        chart.draw_series(LineSeries::new(
            (-314..314).map(|x| x as f64 / 100.0).map(|x| (x, x.sin())),
            &RED
        )).unwrap();
    }
    tracing::info!("{}", buff);


    let div = gloo_utils::document().create_element("div").unwrap();
    div.set_inner_html(&buff[..]);

    Html::VRef(div.into())

    // html! {
    //     <button onclick={Callback::from(|_| {
    //         // spawn_local(async {
    //         //     sleep(Duration::from_millis(100)).await;
    //         // });
    //         // let mut ws = Some(WebSocket::open("ws://localhost:8082/channel").unwrap());
    //         // tracing::info!("{:?}", ws);
    //         ()
    //     })}>
    //         { "Click me!" }
    //     </button>
    // }
}