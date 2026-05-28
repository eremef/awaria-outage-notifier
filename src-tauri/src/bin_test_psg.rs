use scraper::{Html, Selector};
use awaria::api_logic::UnifiedAlert;
use awaria::settings::{Settings, Address};

fn main() {
    let html_content = r#"
        <table>
            <tr>
                <td>LUBUSKIE</td>
                <td>Wschowa</td>
                <td>ul. Wolsztyńska</td>
                <td>2024-05-20</td>
                <td>termin zostanie podany wkrótce</td>
                <td>Awaria gazociągu</td>
                <td>awaria</td>
            </tr>
        </table>
    "#;
    let mut settings = Settings::default();
    settings.addresses.push(Address {
        name: "Dom".to_string(),
        city_name: "Wschowa".to_string(),
        street_name_1: "ul. Wolsztyńska".to_string(),
        street_name_2: None,
        house_no: "1".to_string(),
        voivodeship: "".to_string(),
        district: "".to_string(),
        commune: "".to_string(),
        city_id: 0,
        street_id: 0,
        is_active: true,
    });

    let alerts = awaria::psg::parse_psg_html(html_content, &settings);
    println!("Alerts found: {}", alerts.len());
    for a in alerts {
        println!("{:?}", a);
    }
}
