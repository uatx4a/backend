use std::error::Error;

use afast::AFast;

use backend::{client_target, services};

fn main() -> Result<(), Box<dyn Error>> {
    let registrations = services();
    let targets = registrations
        .iter()
        .map(|(name, _)| client_target(name))
        .collect::<Vec<_>>();
    let mut app = AFast::new();

    for (_, service) in registrations {
        app = app.service(service);
    }

    for target in targets {
        let afast::Lang::JS(calls) = target.lang else {
            unreachable!("client targets are configured for JavaScript");
        };
        app.generate_js(
            &target.path,
            &calls,
            target.debug,
            target.services.as_deref(),
        )?;
    }

    println!("Generated Mini Program API clients under miniprogram/api/");
    Ok(())
}
