/*
 * rustbucket - Navigate AWS S3 buckets in an FTP-like, greppable CLI
 * Copyright © 2020 Jonathan Ming
 *
 * rustbucket is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * rustbucket is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with rustbucket.  If not, see <http://www.gnu.org/licenses/>.
 */

use clap::*;

use rustbucket::Config;

#[tokio::main]
async fn main() {
    let matches = clap::app_from_crate!(", ")
        .arg(
            Arg::with_name("debug")
                .short("d")
                .help("Enable debug logging"),
        )
        .arg(
            Arg::with_name("command")
                .short("c")
                .empty_values(false)
                .value_name("COMMAND")
                .help("Execute a one-off command instead of opening interactive prompt"),
        )
        .get_matches();

    let conf = Config {
        debug: matches.is_present("debug"),
        single_command: matches.value_of("command").map(|s| s.to_owned()),
    };

    println!("rustbucket {}", crate_version!());
    println!(
        "This program comes with ABSOLUTELY NO WARRANTY.
This is free software, and you are welcome to redistribute it.
",
    );

    match rustbucket::run(conf).await {
        Ok(_) => println!("Bye!"),
        Err(e) => eprintln!("Crash: {}", e),
    };
}
