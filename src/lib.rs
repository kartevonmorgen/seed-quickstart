#[macro_use]
extern crate seed;
use futures::Future;
use lazy_static::lazy_static;
use seed::prelude::*;
use semval::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

lazy_static! {
    static ref MAP_ENTRIES: RwLock<Vec<MapEntry>> = RwLock::new(vec![]);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapEntry {
    id: String,
    name: String,
    lat: f64,
    lng: f64,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
struct BBox {
    north_east: LatLng,
    south_west: LatLng,
}

impl BBox {
    fn to_vec(&self) -> Vec<f64> {
        vec![
            self.south_west.lat,
            self.south_west.lng,
            self.north_east.lat,
            self.north_east.lng,
        ]
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
struct LatLng {
    lat: f64,
    lng: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Model {
    pub cities: Option<Vec<City>>,
    pub bbox: Option<BBox>,
    pub selected: Option<Entry>,
    pub entries: Vec<Entry>,
    pub show_new_entry_form: bool,
    pub new_entry_form: EntryFormModel,
    pub new_entry_form_errors: Vec<EntryFormInvalidity>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct EntryFormModel {
    title: String,
    description: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Min(usize);

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Max(usize);

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Actual(usize);

#[derive(Debug, Clone, Serialize, Deserialize)]
enum EntryFormInvalidity {
    TitleLength(Min, Max, Actual),
}

impl Validate for EntryFormModel {
    type Invalidity = EntryFormInvalidity;
    fn validate(&self) -> ValidationResult<Self::Invalidity> {
        ValidationContext::new()
            .invalidate_if(
                self.title.len() < 3,
                EntryFormInvalidity::TitleLength(Min(3), Max(25), Actual(self.title.len())),
            )
            .into()
    }
}

impl Default for Model {
    fn default() -> Self {
        Self {
            cities: None,
            bbox: None,
            selected: None,
            entries: vec![],
            show_new_entry_form: false,
            new_entry_form: EntryFormModel::default(),
            new_entry_form_errors: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NominatimEntry {
    address: NominatimAddress,
    boundingbox: Vec<String>,
    class: String,
    r#type: String,
    display_name: String,
    lat: Option<String>,
    lon: Option<String>,
}

impl NominatimEntry {
    fn is_city(&self) -> bool {
        (self.class == "place" && (self.r#type == "city" || self.r#type == "village"))
            || (self.class == "boundary" && self.r#type == "administrative")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NominatimAddress {
    city: Option<String>,
    country: String,
    country_code: String,
    locality: Option<String>,
    postcode: Option<String>,
    state: Option<String>,
    village: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct City {
    name: String,
    country: String,
    lat: f64,
    lng: f64,
}

fn fetch_nominatim(query: &str) -> impl Future<Item = Msg, Error = Msg> {
    log!("fetch cities");
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&addressdetails=1",
        query
    );
    seed::fetch::Request::new(url)
        .fetch_json_data(|d| Msg::CitySearchResult(d.map_err(|e| format!("{:#?}", e))))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntrySearchResponse {
    pub visible: Vec<Entry>,
    pub invisible: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    pub id: String,
    pub title: String,
    pub description: String,
    pub lat: f64,
    pub lng: f64,
}

fn fetch_entries(bbox: &BBox) -> impl Future<Item = Msg, Error = Msg> {
    let bbox: String = bbox
        .to_vec()
        .into_iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(",");
    log!("fetch entries for {:#?}", bbox);
    let url = format!("https://api.ofdb.io/v0/search?text=&categories=2cd00bebec0c48ba9db761da48678134,77b3c33a92554bcf8e8c2c86cedd6f6f&bbox={}",bbox);
    seed::fetch::Request::new(url)
        .fetch_json_data(|d| Msg::EntrySearchResult(d.map_err(|e| format!("{:#?}", e))))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Msg {
    CitySearch(String),
    CitySearchResult(Result<Vec<NominatimEntry>, String>),
    EntrySearchResult(Result<EntrySearchResponse, String>),
    SetMapCenter(f64, f64),
    UpdateBBox(BBox),
    EntrySelected(String),
    ShowNewEntryForm,
    EntryForm(EntryFormMsg),
    CreateNewEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum EntryFormMsg {
    Title(String),
    Description(String),
}

fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    log!("update");
    match msg {
        Msg::CitySearch(txt) => {
            log!("Send search requrst for '{}'", txt);
            orders.perform_cmd(fetch_nominatim(&txt));
        }
        Msg::CitySearchResult(Ok(res)) => {
            let cities = res
                .iter()
                .filter(|x| x.is_city())
                .map(|c| {
                    (
                        c.address.city.clone(),
                        c.address.country.clone(),
                        c.lat.as_ref(),
                        c.lon.as_ref(),
                    )
                })
                .filter_map(|(name, country, lat, lng)| {
                    if let Some(lat) = lat {
                        if let Some(lng) = lng {
                            if let Some(name) = name {
                                return Some(City {
                                    name,
                                    country,
                                    lat: lat.parse().unwrap(),
                                    lng: lng.parse().unwrap(),
                                });
                            }
                        }
                    }
                    None
                })
                .collect::<Vec<_>>();
            if !cities.is_empty() {
                model.cities = Some(cities);
            }
        }
        Msg::CitySearchResult(Err(fail_reason)) => {
            error!(format!("Fetch error: {:#?}", fail_reason));
        }
        Msg::EntrySearchResult(Ok(res)) => {
            let entries = res
                .visible
                .iter()
                .cloned()
                .map(|e| MapEntry {
                    id: e.id,
                    name: e.title,
                    lat: e.lat,
                    lng: e.lng,
                })
                .collect::<Vec<_>>();
            model.entries = res.visible;
            (*MAP_ENTRIES.write().unwrap()) = entries;
            updateMap();
        }
        Msg::EntrySearchResult(Err(fail_reason)) => {
            error!(format!("Fetch error: {:#?}", fail_reason));
        }
        Msg::SetMapCenter(lat, lng) => {
            log!("New center: {},{}", lat, lng);
            setMapCenter(lat, lng);
            orders.skip();
        }
        Msg::UpdateBBox(bbox) => {
            log!("update bbox in WASM");
            orders.perform_cmd(fetch_entries(&bbox));
            model.bbox = Some(bbox);
        }
        Msg::EntrySelected(id) => {
            log!("entry selected", id);
            model.selected = model.entries.iter().find(|e| e.id == id).cloned();
        }
        Msg::ShowNewEntryForm => {
            model.show_new_entry_form = true;
        }
        Msg::EntryForm(e_msg) => match e_msg {
            EntryFormMsg::Title(txt) => {
                model.new_entry_form.title = txt;
            }
            EntryFormMsg::Description(txt) => {
                model.new_entry_form.description = txt;
            }
        },
        Msg::CreateNewEntry => match model.new_entry_form.validate() {
            Ok(_) => {
                log!("create new entry", model.new_entry_form);
            }
            Err(err) => {
                model.new_entry_form_errors = err.into_iter().collect();
            }
        },
    }
}

fn view(model: &Model) -> impl View<Msg> {
    div![
        h1!["Mapping for Good"],
        input![
            attrs! { At::Type => "text"; At::Placeholder => "which place would you like to discover?";},
            input_ev(Ev::Input, Msg::CitySearch)
        ],
        if model.show_new_entry_form {
            new_entry_form(&model.new_entry_form, &model.new_entry_form_errors)
        } else {
            button![simple_ev(Ev::Click, Msg::ShowNewEntryForm), "add new entry"]
        },
        if let Some(ref cities) = model.cities {
            ul![cities
                .iter()
                .map(|c| li![
                    simple_ev(Ev::Click, Msg::SetMapCenter(c.lat, c.lng)),
                    format!("{},{}", c.name, c.country)
                ])
                .collect::<Vec<_>>()]
        } else {
            seed::empty!()
        },
        if let Some(ref e) = model.selected {
            div![h2![e.title], p![e.description]]
        } else {
            seed::empty!()
        },
    ]
}

fn new_entry_form(m: &EntryFormModel, errors: &[EntryFormInvalidity]) -> Node<Msg> {
    div![
        attrs! {At::Class=>"form"},
        label![
            "Title",
            br![],
            input![
                attrs! {At::Type=>"text"; At::Value=> m.title;},
                input_ev(Ev::Input, |txt| Msg::EntryForm(EntryFormMsg::Title(txt)))
            ],
            if let Some(msg) = errors
                .iter()
                .filter_map(|i| match i {
                    EntryFormInvalidity::TitleLength(min, max, actual) => Some(format!(
                        "Title too short: {} characters, minimum: {}",
                        actual.0, min.0
                    )),
                    _ => None,
                })
                .nth(0)
            {
                div![attrs! {At::Style=>"color:red;"}, msg]
            } else {
                seed::empty()
            }
        ],
        br![],
        label![
            "Description",
            br![],
            textarea![
                input_ev(Ev::Input, |txt| Msg::EntryForm(EntryFormMsg::Description(
                    txt
                ))),
                m.description,
            ],
        ],
        br![],
        button![simple_ev(Ev::Click, Msg::CreateNewEntry), "create"]
    ]
}

#[wasm_bindgen(start)]
pub fn render() {
    seed::App::build(|_, _| Model::default(), update, view)
        .window_events(window_events)
        .finish()
        .run();
}

fn window_events(model: &Model) -> Vec<seed::events::Listener<Msg>> {
    let mut result = Vec::new();
    result.push(seed::events::trigger_update_handler());
    result
}

#[wasm_bindgen]
pub fn get_map_entries() -> JsValue {
    log!("get map entries");
    JsValue::from_serde(&*MAP_ENTRIES.read().unwrap()).unwrap()
}

#[wasm_bindgen]
pub fn update_bbox(
    north_east_lat: f64,
    north_east_lng: f64,
    south_west_lat: f64,
    south_west_lng: f64,
) {
    log!(
        "Got bounds from JS: {}{}{}{}",
        north_east_lat,
        north_east_lng,
        south_west_lat,
        south_west_lng,
    );
    let bbox = BBox {
        north_east: LatLng {
            lat: north_east_lat,
            lng: north_east_lng,
        },
        south_west: LatLng {
            lat: south_west_lat,
            lng: south_west_lng,
        },
    };
    log!("update seed app");
    seed::set_timeout(
        Box::new(move || {
            seed::update(Msg::UpdateBBox(bbox));
        }),
        15,
    );
}

#[wasm_bindgen]
pub fn marker_clicked(id: String) {
    log!("marker {}", id);
    seed::update(Msg::EntrySelected(id));
}

#[wasm_bindgen]
extern "C" {
    fn setMapCenter(lat: f64, lng: f64);
    fn updateMap();
}
