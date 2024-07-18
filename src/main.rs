use std::collections::HashSet;
use clap::Parser;
use imap::types::Uid;
use mailparse::DispositionType::Attachment;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Options {
    #[arg(short, long)]
    server: String,

    #[arg(short, long)]
    username: String,

    #[arg(short, long)]
    password: String,

    #[arg(short, long, default_value = "INBOX")]
    mailbox: String,
}

///
/// Turn a HashSet with UIDs into a comma separated String
///
fn create_uidset(uids: &HashSet<Uid>) -> String {
    uids.iter()
        .map(|uid| uid.to_string())
        .fold(String::new(), |mut a, b| {
            if !a.is_empty() {
                a.push(',');
            }
            a.push_str(&b);
            a
        })
}


fn main() -> Result<(), anyhow::Error>{
    let options = Options::parse();

    let tls = native_tls::TlsConnector::builder().build()?;
    let client = imap::connect_starttls((options.server.as_str(), 143), &options.server, &tls)?;
    let mut session = client.login(&options.username, &options.password).expect("Failed IMAP login");

    let mailbox = session.select(&options.mailbox)?;
    assert!(mailbox.uid_validity.is_some());

    let uids = session.uid_search("ALL")?;
    dbg!(&uids);

    let messages = session.uid_fetch(create_uidset(&uids), "BODY[]")?;
    for message in messages.iter() {
        let body = message.body().expect("Message without a body");
        let mail = mailparse::parse_mail(body)?;
        for part in mail.subparts {
            let content_disp = part.headers.iter().find(|header| header.get_key_ref() == "Content-Disposition");
            if let Some(content_disp_header) = content_disp {
                let disposition = mailparse::parse_content_disposition(&content_disp_header.get_value());

                if disposition.disposition == Attachment {
                    let filename = disposition.params.get("filename");
                    if let Some(filename) = filename {
                        dbg!(filename);
                        dbg!(part.raw_bytes.len());
                    }
                }
            }
        }
    }


    Ok(())
}
