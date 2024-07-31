use std::collections::{HashMap, HashSet};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;
use clap::Parser;
use imap::Session;
use imap::types::Uid;
use mailparse::DispositionType::Attachment;
use mailparse::ParsedMail;
use native_tls::TlsStream;

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

    #[arg(short, long)]
    output: PathBuf,
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

///
/// Go through the subparts of an email looking for anything that looks like a file,
/// as indicated by the Content-Disposition header with a filename being present.
///
/// returns a HashMap<filename, file_contents>
fn extract_attachments(mail: &ParsedMail) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    let mut attachments: HashMap<String, Vec<u8>> = HashMap::new();

    for part in &mail.subparts {
        let content_disp = part.headers.iter().find(|header| header.get_key_ref() == "Content-Disposition");
        if let Some(content_disp_header) = content_disp {
            let disposition = mailparse::parse_content_disposition(&content_disp_header.get_value());

            if disposition.disposition == Attachment {
                let filename = disposition.params.get("filename");
                if let Some(filename) = filename {
                    dbg!(filename);
                    dbg!(part.get_body_raw()?.len());
                    attachments.insert(filename.clone(), part.get_body_raw()?);
                }
            }
        }
    }

    Ok(attachments)
}


fn make_unique_file(dir: &Path, base_filename: &str) -> PathBuf {
    if !dir.join(base_filename).exists() {
        dir.join(base_filename)
    } else {
        let mut i = 0u32;
        while dir.join(format!("{i}-{base_filename}")).exists() {
            i += 1;
        }
        dir.join(format!("{i}-{base_filename}"))
    }
}

///
/// Find all new messages in the mailbox,
/// read their attachments,
/// download them to the output directory
/// and remove the messages from the server
///
fn handle_mail(session: &mut Session<TlsStream<TcpStream>>, output: &Path) -> anyhow::Result<()> {
    session.noop()?; // refresh

    let uids = session.uid_search("ALL")?;
    dbg!(&uids);

    if uids.is_empty() {
        return Ok(())
    }

    let messages = session.uid_fetch(create_uidset(&uids), "BODY[]")?;
    for message in messages.iter() {
        let body = message.body().expect("Message without a body");
        let mail = mailparse::parse_mail(body)?;
        let attachments = extract_attachments(&mail)?;

        for (filename, contents) in attachments {
            let final_file = make_unique_file(output, &filename);
            std::fs::write(final_file, &contents)?;
        }
    }

    session.uid_store(create_uidset(&uids), "+FLAGS.SILENT (\\Deleted)")?;
    session.expunge()?;
    Ok(())
}


fn main() -> Result<(), anyhow::Error>{
    let options = Options::parse();

    let tls = native_tls::TlsConnector::builder().build()?;
    let client = imap::connect_starttls((options.server.as_str(), 143), &options.server, &tls)?;
    let mut session = client.login(&options.username, &options.password).expect("Failed IMAP login");
    assert!(session.capabilities()?.has_str("IDLE"));

    let mailbox = session.select(&options.mailbox)?;
    assert!(mailbox.uid_validity.is_some());

    loop {
        handle_mail(&mut session, &options.output)?;
        session.idle().unwrap().wait_with_timeout(Duration::from_secs(60))?;
    }
}
