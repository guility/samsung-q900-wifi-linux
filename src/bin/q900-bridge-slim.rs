use std::collections::HashMap;
use std::env;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
enum SlimValue {
    Text(String),
    List(Vec<SlimValue>),
}

#[derive(Default)]
struct Session {
    fixtures: HashMap<String, BridgeFixture>,
    symbols: HashMap<String, String>,
}

#[derive(Default)]
struct BridgeFixture;

fn main() {
    if let Err(error) = run() {
        eprintln!("q900-bridge-slim: {error}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let port = env::args().nth(1).unwrap_or_else(|| "1".to_owned());
    if port == "1" {
        let stdin = io::stdin();
        let stdout = io::stdout();
        return serve(&mut stdin.lock(), &mut stdout.lock());
    }

    let port = port.parse::<u16>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid Slim port: {error}"),
        )
    })?;
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let (mut stream, _) = listener.accept()?;
    let mut reader = stream.try_clone()?;
    serve(&mut reader, &mut stream)
}

fn serve<R: Read, W: Write>(incoming: &mut R, outgoing: &mut W) -> io::Result<()> {
    outgoing.write_all(b"Slim -- V0.5\n")?;
    outgoing.flush()?;
    let mut session = Session::default();

    while let Some(message) = read_frame(incoming)? {
        if message.eq_ignore_ascii_case("bye") {
            break;
        }

        let instructions = parse_slim_list(message.as_bytes()).map_err(invalid_data)?;
        let response = session.execute(instructions);
        let payload = serialize(&SlimValue::List(response));
        write_frame(outgoing, &payload)?;
    }
    Ok(())
}

fn read_frame<R: Read>(incoming: &mut R) -> io::Result<Option<String>> {
    let mut prefix = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        match incoming.read(&mut byte)? {
            0 if prefix.is_empty() => return Ok(None),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Slim frame ended inside its length prefix",
                ));
            }
            _ if byte[0] == b':' => break,
            _ if byte[0].is_ascii_digit() => prefix.push(byte[0]),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Slim frame length contains a non-digit",
                ));
            }
        }
    }

    let length = std::str::from_utf8(&prefix)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Slim frame length"))?;
    let mut payload = vec![0_u8; length];
    incoming.read_exact(&mut payload)?;
    String::from_utf8(payload)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_frame<W: Write>(outgoing: &mut W, payload: &[u8]) -> io::Result<()> {
    write!(outgoing, "{:06}:", payload.len())?;
    outgoing.write_all(payload)?;
    outgoing.flush()
}

fn parse_slim_list(input: &[u8]) -> Result<SlimValue, String> {
    let (value, end) = parse_list_at(input, 0)?;
    if end != input.len() {
        return Err("trailing bytes after Slim list".to_owned());
    }
    Ok(value)
}

fn parse_list_at(input: &[u8], mut index: usize) -> Result<(SlimValue, usize), String> {
    if input.get(index) != Some(&b'[') {
        return Err("Slim list does not start with '['".to_owned());
    }
    index += 1;
    let count = parse_length(input, &mut index)?;
    let mut values = Vec::with_capacity(count);

    for _ in 0..count {
        let length = parse_length(input, &mut index)?;
        let end = index
            .checked_add(length)
            .filter(|end| *end <= input.len())
            .ok_or_else(|| "Slim item length exceeds its message".to_owned())?;
        let item = &input[index..end];
        index = end;
        if input.get(index) != Some(&b':') {
            return Err("Slim item is missing its trailing ':'".to_owned());
        }
        index += 1;

        if item.starts_with(b"[") {
            let (list, parsed) = parse_list_at(item, 0)?;
            if parsed != item.len() {
                return Err("trailing bytes after nested Slim list".to_owned());
            }
            values.push(list);
        } else {
            let text = std::str::from_utf8(item)
                .map_err(|error| format!("Slim item is not UTF-8: {error}"))?;
            values.push(SlimValue::Text(text.to_owned()));
        }
    }

    if input.get(index) != Some(&b']') {
        return Err("Slim list is missing its closing ']'".to_owned());
    }
    Ok((SlimValue::List(values), index + 1))
}

fn parse_length(input: &[u8], index: &mut usize) -> Result<usize, String> {
    let start = *index;
    while input.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    if *index - start < 6 || input.get(*index) != Some(&b':') {
        return Err("invalid Slim length field".to_owned());
    }
    let length = std::str::from_utf8(&input[start..*index])
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| "invalid Slim length value".to_owned())?;
    *index += 1;
    Ok(length)
}

fn serialize(value: &SlimValue) -> Vec<u8> {
    match value {
        SlimValue::Text(text) => text.as_bytes().to_vec(),
        SlimValue::List(items) => {
            let mut bytes = Vec::new();
            bytes.push(b'[');
            append_length(&mut bytes, items.len());
            for item in items {
                let item = serialize(item);
                append_length(&mut bytes, item.len());
                bytes.extend_from_slice(&item);
                bytes.push(b':');
            }
            bytes.push(b']');
            bytes
        }
    }
}

fn append_length(bytes: &mut Vec<u8>, length: usize) {
    bytes.extend_from_slice(format!("{length:06}:").as_bytes());
}

impl Session {
    fn execute(&mut self, request: SlimValue) -> Vec<SlimValue> {
        let SlimValue::List(instructions) = request else {
            return vec![response("0", "__EXCEPTION__:expected instruction list")];
        };
        instructions
            .iter()
            .map(|instruction| self.execute_instruction(instruction))
            .collect()
    }

    fn execute_instruction(&mut self, instruction: &SlimValue) -> SlimValue {
        let SlimValue::List(fields) = instruction else {
            return response("0", "__EXCEPTION__:expected instruction");
        };
        let id = text(fields, 0).unwrap_or("0").to_owned();
        let operation = text(fields, 1).unwrap_or_default();
        let result = match operation {
            "import" => "OK".to_owned(),
            "make" => self.make_fixture(fields),
            "call" => self.call(fields, 2, 3, 4),
            "callAndAssign" => self.call_and_assign(fields),
            "assign" => self.assign(fields),
            _ => format!("__EXCEPTION__:unsupported Slim instruction {operation}"),
        };
        response(&id, &result)
    }

    fn make_fixture(&mut self, fields: &[SlimValue]) -> String {
        let Some(instance) = text(fields, 2) else {
            return "__EXCEPTION__:missing Slim fixture instance name".to_owned();
        };
        let Some(class) = text(fields, 3) else {
            return "__EXCEPTION__:missing Slim fixture class name".to_owned();
        };
        if normalize(class) != "q900bridgefixture" {
            return format!("__EXCEPTION__:unknown fixture {class}");
        }
        self.fixtures.insert(instance.to_owned(), BridgeFixture);
        "OK".to_owned()
    }

    fn call(
        &mut self,
        fields: &[SlimValue],
        instance_at: usize,
        method_at: usize,
        args_at: usize,
    ) -> String {
        let Some(instance) = text(fields, instance_at) else {
            return "__EXCEPTION__:missing Slim fixture instance".to_owned();
        };
        let Some(method) = text(fields, method_at) else {
            return "__EXCEPTION__:missing Slim fixture method".to_owned();
        };
        let args: Vec<SlimValue> = fields[args_at.min(fields.len())..]
            .iter()
            .map(|value| self.resolve(value))
            .collect();
        let Some(fixture) = self.fixtures.get(instance) else {
            return format!("__EXCEPTION__:unknown fixture instance {instance}");
        };
        fixture.call(method, &args)
    }

    fn call_and_assign(&mut self, fields: &[SlimValue]) -> String {
        let Some(symbol) = text(fields, 2).map(str::to_owned) else {
            return "__EXCEPTION__:missing Slim symbol".to_owned();
        };
        let result = self.call(fields, 3, 4, 5);
        if !result.starts_with("__EXCEPTION__:") {
            self.symbols.insert(symbol, result.clone());
        }
        result
    }

    fn assign(&mut self, fields: &[SlimValue]) -> String {
        let Some(symbol) = text(fields, 2).map(str::to_owned) else {
            return "__EXCEPTION__:missing Slim symbol".to_owned();
        };
        let Some(value) = text(fields, 3).map(str::to_owned) else {
            return "__EXCEPTION__:missing Slim symbol value".to_owned();
        };
        self.symbols.insert(symbol, value);
        "OK".to_owned()
    }

    fn resolve(&self, value: &SlimValue) -> SlimValue {
        match value {
            SlimValue::Text(text) => text
                .strip_prefix('$')
                .and_then(|symbol| self.symbols.get(symbol))
                .map(|value| SlimValue::Text(value.clone()))
                .unwrap_or_else(|| value.clone()),
            SlimValue::List(_) => value.clone(),
        }
    }
}

impl BridgeFixture {
    fn call(&self, method: &str, args: &[SlimValue]) -> String {
        match normalize(method).as_str() {
            "senderplancontains" => {
                if args.len() != 2 {
                    return "__EXCEPTION__:senderPlanContains expects layout and text".to_owned();
                }
                let (Some(layout), Some(expected)) = (text(args, 0), text(args, 1)) else {
                    return "__EXCEPTION__:senderPlanContains arguments must be text".to_owned();
                };
                match sender_dry_run(layout) {
                    Ok(output) => output.contains(expected).to_string(),
                    Err(error) => format!("__EXCEPTION__:{error}"),
                }
            }
            "helpcontains" => {
                if args.len() != 1 {
                    return "__EXCEPTION__:helpContains expects one text argument".to_owned();
                }
                let Some(expected) = text(args, 0) else {
                    return "__EXCEPTION__:helpContains argument must be text".to_owned();
                };
                match bridge_binary().args(["--help"]).output() {
                    Ok(output) if output.status.success() => {
                        String::from_utf8_lossy(&output.stdout)
                            .contains(expected)
                            .to_string()
                    }
                    Ok(output) => format!("__EXCEPTION__:CLI exited with {}", output.status),
                    Err(error) => format!("__EXCEPTION__:could not run q900-bridge: {error}"),
                }
            }
            _ => format!("__EXCEPTION__:unknown fixture method {method}"),
        }
    }
}

fn sender_dry_run(layout: &str) -> Result<String, String> {
    let output = bridge_binary()
        .args([
            "sender",
            "--layout",
            layout,
            "--destination-ip",
            "239.0.0.56",
            "--local-sink",
            "alsa_output.acceptance",
            "--dry-run",
        ])
        .output()
        .map_err(|error| format!("could not run q900-bridge: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "q900-bridge exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

fn bridge_binary() -> Command {
    Command::new(
        env::var_os("Q900_BRIDGE_BIN").unwrap_or_else(|| "target/debug/q900-bridge".into()),
    )
}

fn text(values: &[SlimValue], index: usize) -> Option<&str> {
    match values.get(index)? {
        SlimValue::Text(text) => Some(text),
        SlimValue::List(_) => None,
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn response(id: &str, result: &str) -> SlimValue {
    SlimValue::List(vec![
        SlimValue::Text(id.to_owned()),
        SlimValue::Text(result.to_owned()),
    ])
}

fn invalid_data(error: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::{SlimValue, parse_slim_list, serialize};

    #[test]
    fn round_trips_nested_slim_instructions() {
        let message = SlimValue::List(vec![SlimValue::List(vec![
            SlimValue::Text("1".to_owned()),
            SlimValue::Text("call".to_owned()),
            SlimValue::Text("scriptTableActor".to_owned()),
            SlimValue::Text("senderPlanContains".to_owned()),
            SlimValue::Text("7.1".to_owned()),
            SlimValue::Text("channels=8".to_owned()),
        ])]);

        assert_eq!(parse_slim_list(&serialize(&message)).unwrap(), message);
    }

    #[test]
    fn rejects_truncated_slim_items() {
        assert!(parse_slim_list(b"[000001000005:hi:]").is_err());
    }
}
