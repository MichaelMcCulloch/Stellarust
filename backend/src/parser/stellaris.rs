use nom::{
    branch::alt,
    bytes::complete::{escaped, is_a, tag, tag_no_case, take_while},
    character::{
        complete::{alphanumeric0, alphanumeric1, anychar, char, digit0, digit1, one_of},
        is_alphanumeric,
    },
    combinator::{cut, map, map_res, opt, recognize, value},
    error::{context, ErrorKind, ParseError, VerboseError},
    multi::separated_list0,
    number::complete::{self, double as parse_double, double, i64 as parse_i64},
    sequence::{delimited, preceded, separated_pair, terminated, tuple},
    AsChar, IResult, InputTakeAtPosition,
};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    hash::Hash,
    io::BufRead,
};
use time::{Date, Month};

type Res<T, S> = IResult<T, S, VerboseError<T>>;
#[derive(Debug, Clone, PartialEq)]
pub struct DateParseError {
    err: String,
}

impl Error for DateParseError {}

impl Display for DateParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.err, f)
    }
}

#[derive(Debug, PartialEq)]
pub enum Val<'a> {
    Boolean(bool),
    String(&'a str),
    Integer(i64),
    Decimal(f64),
    Dict(HashMap<&'a str, Vec<Val<'a>>>),
    Array(Vec<Val<'a>>),
    Set(Vec<Val<'a>>),
    Date(Date),
}

fn space<'a>(i: &'a str) -> Res<&'a str, &'a str> {
    let chars = " \t\r\n";
    context("space", take_while(move |c| chars.contains(c)))(i)
}

fn boolean<'a>(input: &'a str) -> Res<&'a str, bool> {
    let parse_true = value(true, tag_no_case("yes"));
    let parse_false = value(false, tag_no_case("no"));
    alt((parse_true, parse_false))(input)
}

fn integer<'a>(i: &'a str) -> Res<&'a str, i64> {
    context(
        "integer",
        map_res(recognize(tuple((opt(char('-')), digit1))), str::parse),
    )(i)
}

fn decimal<'a>(i: &'a str) -> Res<&'a str, f64> {
    context(
        "decimal",
        map_res(
            recognize(tuple((opt(char('-')), digit1, char('.'), digit1))),
            str::parse,
        ),
    )(i)
}

fn key<'a>(i: &'a str) -> Res<&'a str, &'a str> {
    take_while(move |c: char| c.is_alphabetic() || c == '_')(i)
}

fn raw_string<'a>(i: &'a str) -> Res<&'a str, &'a str> {
    let chars = "\"=";
    context(
        "raw_string",
        take_while(move |c: char| {
            !chars.contains(c)
                && (c.is_alphanumeric() || c.is_whitespace() || c.is_ascii_punctuation())
        }),
    )(i)
}

fn raw_date<'a>(i: &'a str) -> Res<&'a str, Date> {
    context(
        "raw_date",
        map_res(
            recognize(tuple((digit1, char('.'), digit1, char('.'), digit1))),
            map_to_date,
        ),
    )(i)
}

fn key_value<'a>(i: &'a str) -> Res<&'a str, (&'a str, Val<'a>)> {
    context(
        "key_value",
        separated_pair(
            preceded(space, key),
            preceded(space, char('=')),
            preceded(space, val),
        ),
    )(i)
}

fn indexed_value<'a>(i: &'a str) -> Res<&'a str, (usize, Val<'a>)> {
    context(
        "indexed_value",
        separated_pair(
            preceded(space, map_res(recognize(digit1), str::parse)),
            preceded(space, char('=')),
            preceded(space, val),
        ),
    )(i)
}

fn quoted_date<'a>(i: &'a str) -> Res<&'a str, Date> {
    context(
        "quoted_date",
        preceded(char('\"'), terminated(raw_date, char('\"'))),
    )(i)
}

fn quoted_string<'a>(i: &'a str) -> Res<&'a str, &'a str> {
    context(
        "quoted_string",
        preceded(char('\"'), terminated(raw_string, char('\"'))),
    )(i)
}

fn array<'a>(i: &'a str) -> Res<&'a str, Vec<(usize, Val<'a>)>> {
    context(
        "array",
        preceded(
            char('{'),
            terminated(
                separated_list0(space, indexed_value),
                preceded(space, char('}')),
            ),
        ),
    )(i)
}
fn dict<'a>(i: &'a str) -> Res<&'a str, Vec<(&str, Val<'a>)>> {
    context(
        "dict",
        preceded(
            char('{'),
            terminated(
                separated_list0(space, key_value),
                preceded(space, char('}')),
            ),
        ),
    )(i)
}
fn set<'a>(i: &'a str) -> Res<&'a str, Vec<Val<'a>>> {
    context(
        "set",
        preceded(
            char('{'),
            terminated(separated_list0(space, val), preceded(space, char('}'))),
        ),
    )(i)
}

fn val<'a>(input: &'a str) -> Res<&'a str, Val<'a>> {
    context(
        "val",
        preceded(
            space,
            alt((
                map(array, |pairs| Val::Array(fold_into_array(pairs))),
                map(dict, |tuple| Val::Dict(fold_into_hashmap(tuple))),
                map(set, |set| Val::Set(set)),
                map(quoted_date, Val::Date),
                map(quoted_string, Val::String),
                map(boolean, Val::Boolean),
                map(decimal, Val::Decimal),
                map(integer, Val::Integer),
            )),
        ),
    )(input)
}

fn root<'a>(i: &'a str) -> Res<&'a str, Val<'a>> {
    context(
        "root",
        preceded(
            space,
            terminated(
                map(
                    map(separated_list0(space, key_value), fold_into_hashmap),
                    Val::Dict,
                ),
                space,
            ),
        ),
    )(i)
}

fn map_to_date<'a>(s: &'a str) -> anyhow::Result<Date> {
    let parts: Vec<&'a str> = s.split(".").collect();

    let year: i32 = parts
        .get(0)
        .ok_or(DateParseError {
            err: String::from("Too Short"),
        })?
        .parse()?;
    let month = match *parts.get(1).ok_or(DateParseError {
        err: String::from("Too Short"),
    })? {
        "01" => Ok(Month::January),
        "02" => Ok(Month::February),
        "03" => Ok(Month::March),
        "04" => Ok(Month::April),
        "05" => Ok(Month::May),
        "06" => Ok(Month::June),
        "07" => Ok(Month::July),
        "08" => Ok(Month::August),
        "09" => Ok(Month::September),
        "10" => Ok(Month::October),
        "11" => Ok(Month::November),
        "12" => Ok(Month::December),
        _ => Err(DateParseError {
            err: String::from("Months beyond December are not supported, dummy!"),
        }),
    };
    let day: u8 = parts
        .get(2)
        .ok_or(DateParseError {
            err: String::from("Too Short"),
        })?
        .parse()?;
    Ok(Date::from_calendar_date(year, month?, day)?)
}
fn fold_into_array<'a>(tuple_vec: Vec<(usize, Val<'a>)>) -> Vec<Val<'a>> {
    tuple_vec
        .into_iter()
        .fold(Vec::new(), |mut acc, (index, value)| {
            acc.push(value);
            acc
        })
}
fn fold_into_hashmap<'a>(tuple_vec: Vec<(&'a str, Val<'a>)>) -> HashMap<&'a str, Vec<Val<'a>>> {
    tuple_vec
        .into_iter()
        .fold(HashMap::new(), |mut acc, (key, value)| {
            {
                let entry = acc.entry(key).or_insert(vec![]);
                entry.push(value)
            }
            acc
        })
}
#[cfg(test)]
mod tests {
    use core::panic;
    use std::path::PathBuf;

    use std::fs;

    use super::*;
    #[test]
    fn space__captures_all_spaces() {
        let text = " \t\n\r";
        let (_, actual) = space(text).unwrap();
        assert_eq!(actual, text);
    }

    #[test]
    fn raw_string__simple_word__parses_word() {
        let text = "Text";
        let (_, actual) = raw_string(text).unwrap();
        assert_eq!(actual, text);
    }

    #[test]
    fn raw_string__two_words__parses_word() {
        let text = "Text part";
        let (_, actual) = raw_string(text).unwrap();
        assert_eq!(actual, text);
    }

    #[test]
    fn raw_string__two_words_and_numbers__parses_word() {
        let text = "Text part 2";
        let (_, actual) = raw_string(text).unwrap();
        assert_eq!(actual, text);
    }

    #[test]
    fn raw_string__two_words_numbers_and_period__parses_word() {
        let text = "Text part 2.";
        let (_, actual) = raw_string(text).unwrap();
        assert_eq!(actual, text);
    }

    #[test]
    fn raw_string__two_words_numbers_period_and_symbol__parses_word() {
        let text = "Text part 2.~!@#$%^&*()_+`1234567890-[];',./{}|:<>?";
        let (_, actual) = raw_string(text).unwrap();
        assert_eq!(actual, text);
    }

    #[test]
    fn quoted_string__simple_word__parses_word() {
        let text = "\"Text\"";
        let (_, actual) = quoted_string(text).unwrap();
        assert_eq!(actual, "Text");
    }

    #[test]
    fn quoted_string__two_words__parses_word() {
        let text = "\"Text part\"";
        let (_, actual) = quoted_string(text).unwrap();
        assert_eq!(actual, "Text part");
    }

    #[test]
    fn quoted_string__two_words_and_numbers__parses_word() {
        let text = "\"Text part 2\"";
        let (_, actual) = quoted_string(text).unwrap();
        assert_eq!(actual, "Text part 2");
    }

    #[test]
    fn quoted_string__two_words_numbers_and_period__parses_word() {
        let text = "\"Text part 2.\"";
        let (_, actual) = quoted_string(text).unwrap();
        assert_eq!(actual, "Text part 2.");
    }

    #[test]
    fn quoted_string__two_words_numbers_period_and_symbol__parses_word() {
        let text = "\"Text part 2.~!@#$%^&*()_+`1234567890-[];',./{}|:<>?\"";
        let (_, actual) = quoted_string(text).unwrap();
        assert_eq!(
            actual,
            "Text part 2.~!@#$%^&*()_+`1234567890-[];',./{}|:<>?"
        );
    }

    #[test]
    fn quoted_string__quoted_numbers__parses_word() {
        let text = "\"-7.2\"";
        let (_, actual) = quoted_string(text).unwrap();
        assert_eq!(actual, "-7.2");
    }

    #[test]
    fn integer__zero__returns_0() {
        let text = "0";
        let (_, actual) = integer(text).unwrap();
        assert_eq!(actual, 0i64);
    }

    #[test]
    fn integer__negative__returns_negative_number() {
        let text = "-1";
        let (_, actual) = integer(text).unwrap();
        assert_eq!(actual, -1i64);
    }

    #[test]
    fn integer__a_really_big_number__returns_number() {
        let text = "1234567";
        let (_, actual) = integer(text).unwrap();
        assert_eq!(actual, 1234567i64);
    }

    #[test]
    fn integer__leading0__returns_number() {
        let text = "01234567";
        let (_, actual) = integer(text).unwrap();
        assert_eq!(actual, 1234567i64);
    }

    #[test]
    fn decimal__zero__returns_0() {
        let text = "0.0";
        let (_, actual) = decimal(text).unwrap();
        assert_eq!(actual, 0.0f64);
    }

    #[test]
    fn decimal__negative__returns_negative_number() {
        let text = "-1.0";
        let (_, actual) = decimal(text).unwrap();
        assert_eq!(actual, -1.0f64);
    }

    #[test]
    fn decimal__a_really_big_number__returns_number() {
        let text = "1234567.0";
        let (_, actual) = decimal(text).unwrap();
        assert_eq!(actual, 1234567.0f64);
    }

    #[test]
    fn decimal__leading0__returns_number() {
        let text = "01234567.0";
        let (_, actual) = decimal(text).unwrap();
        assert_eq!(actual, 1234567.0f64);
    }

    #[test]
    fn key_value__simple_string_assignment__returns_key_value_pair() {
        let text = "name=\"Empire\"";
        let (_, (key, val)) = key_value(text).unwrap();

        assert_eq!(key, "name");
        assert_eq!(val, Val::String("Empire"));
    }

    #[test]
    fn key_value__simple_integer_assignment__returns_key_value_pair() {
        let text = "name=0";
        let (_, (key, val)) = key_value(text).unwrap();

        assert_eq!(key, "name");
        assert_eq!(val, Val::Integer(0));
    }

    #[test]
    fn key_value__simple_decimal_assignment__returns_key_value_pair() {
        let text = "name=-0.7";
        let (_, (key, val)) = key_value(text).unwrap();

        assert_eq!(key, "name");
        assert_eq!(val, Val::Decimal(-0.7));
    }

    #[test]
    fn key_value__simple_date_assignment__returns_key_value_pair() {
        let text = "name=\"2200.01.01\"";
        let (_, (key, val)) = key_value(text).unwrap();

        assert_eq!(key, "name");
        assert_eq!(
            val,
            Val::Date(Date::from_calendar_date(2200, Month::January, 01).unwrap())
        );
    }

    #[test]
    fn key_value__simple_boolean_assignment__returns_key_value_pair() {
        let text = "name=no";
        let (_, (key, val)) = key_value(text).unwrap();

        assert_eq!(key, "name");
        assert_eq!(val, Val::Boolean(false));
    }

    #[test]
    fn key_value__dict_assignment__returns_key_value_pair() {
        let text = "name={first=\"Bond\"\nsecond=\"James Bond\"\n}\n";
        let (_, (key, val)) = key_value(text).unwrap();

        assert_eq!(key, "name");

        if let Val::Dict(dict) = val {
            assert!(dict.contains_key("first"));
            assert!(dict.contains_key("second"));

            let first = dict.get("first").unwrap();
            let second = dict.get("second").unwrap();

            assert_eq!(first, &vec![Val::String("Bond")]);
            assert_eq!(second, &vec![Val::String("James Bond")]);
        } else {
            let mut string = String::from("Expected an Dict, but received a ");

            string.push_str(match val {
                Val::Boolean(_) => "Boolean",
                Val::String(_) => "String",
                Val::Integer(_) => "Integer",
                Val::Decimal(_) => "Decimal",
                Val::Dict(_) => "Dict",
                Val::Array(_) => "Array",
                Val::Set(_) => "Set",
                Val::Date(_) => "Date",
            });
            panic!("{}", string);
        }
    }

    #[test]
    fn key_value__array_assignment__returns_ordered_array() {
        let text = r###"name={
0="Bond"
1="James Bond"
}"###;
        let (_, (key, val)) = key_value(text).unwrap();

        assert_eq!(key, "name");

        if let Val::Array(vec) = val {
            assert_eq!(vec.len(), 2);

            let first = vec.get(0).unwrap();
            let second = vec.get(1).unwrap();

            assert_eq!(first, &Val::String("Bond"));
            assert_eq!(second, &Val::String("James Bond"));
        } else {
            let mut string = String::from("Expected an array, but received a ");

            string.push_str(match val {
                Val::Boolean(_) => "Boolean",
                Val::String(_) => "String",
                Val::Integer(_) => "Integer",
                Val::Decimal(_) => "Decimal",
                Val::Dict(_) => "Dict",
                Val::Array(_) => "Array",
                Val::Set(_) => "Set",
                Val::Date(_) => "Date",
            });
            panic!("{}", string);
        }
    }

    #[test]
    fn key_value__set_assignment__returns_ordered_array() {
        let text = r###"name={
"Bond"
"James Bond"
}"###;
        let (_, (key, val)) = key_value(text).unwrap();

        assert_eq!(key, "name");

        if let Val::Set(vec) = val {
            assert_eq!(vec.len(), 2);

            assert!(vec.contains(&Val::String("Bond")));
            assert!(vec.contains(&Val::String("James Bond")));
        } else {
            let mut string = String::from("Expected an Set, but received a ");

            string.push_str(match val {
                Val::Boolean(_) => "Boolean",
                Val::String(_) => "String",
                Val::Integer(_) => "Integer",
                Val::Decimal(_) => "Decimal",
                Val::Dict(_) => "Dict",
                Val::Array(_) => "Array",
                Val::Set(_) => "Set",
                Val::Date(_) => "Date",
            });
            panic!("{}", string);
        }
    }

    #[test]
    fn raw_date__simple_raw_date__returns_date() {
        let text = "2021.12.23";
        let (_, actual) = raw_date(text).unwrap();
        assert_eq!(
            actual,
            Date::from_calendar_date(2021, Month::December, 23).unwrap()
        );
    }

    #[test]
    fn raw_date__min_values___returns_date() {
        let text = "0000.01.01";
        let (_, actual) = raw_date(text).unwrap();
        assert_eq!(
            actual,
            Date::from_calendar_date(0, Month::January, 01).unwrap()
        );
    }

    #[test]
    fn raw_date__max_values___returns_date() {
        let text = "9999.12.31";
        let (_, actual) = raw_date(text).unwrap();
        assert_eq!(
            actual,
            Date::from_calendar_date(9999, Month::December, 31).unwrap()
        );
    }

    #[test]
    fn quoted_date__simple_quoted_date__returns_date() {
        let text = "\"2021.12.23\"";
        let (_, actual) = quoted_date(text).unwrap();
        assert_eq!(
            actual,
            Date::from_calendar_date(2021, Month::December, 23).unwrap()
        );
    }

    #[test]
    fn quoted_date__min_values___returns_date() {
        let text = "\"0000.01.01\"";
        let (_, actual) = quoted_date(text).unwrap();
        assert_eq!(
            actual,
            Date::from_calendar_date(0, Month::January, 01).unwrap()
        );
    }

    #[test]
    fn quoted_date__max_values___returns_date() {
        let text = "\"9999.12.31\"";
        let (_, actual) = quoted_date(text).unwrap();
        assert_eq!(
            actual,
            Date::from_calendar_date(9999, Month::December, 31).unwrap()
        );
    }

    #[test]
    fn bool__given_no__returns_false() {
        let text = "no";
        let (_, actual) = boolean(text).unwrap();
        assert_eq!(actual, false);
    }

    #[test]
    fn bool__given_yes__returns_true() {
        let text = "yes";
        let (_, actual) = boolean(text).unwrap();
        assert_eq!(actual, true);
    }

    #[test]
    fn set__new_line_separated_strings__returns_array_of_strings() {
        let text = "{\"hello\"\n\"world\"\n}";
        let (_, actual) = set(text).unwrap();
        assert_eq!(actual, vec![Val::String("hello"), Val::String("world")]);
    }

    #[test]
    fn set__new_line_separated_dates__returns_array_of_strings() {
        let text = "{\"2200.01.01\"\n\"2200.01.01\"\n}";
        let (_, actual) = set(text).unwrap();
        assert_eq!(
            actual,
            vec![
                Val::Date(Date::from_calendar_date(2200, Month::January, 01).unwrap()),
                Val::Date(Date::from_calendar_date(2200, Month::January, 01).unwrap())
            ]
        );
    }

    #[test]
    fn set__new_line_separated_numbers__returns_array_of_strings() {
        let text = "{2200\n-7.2\n}";
        let (_, actual) = set(text).unwrap();
        assert_eq!(actual, vec![Val::Integer(2200), Val::Decimal(-7.2)]);
    }

    #[test]
    fn array__indexed_dates__returns_array_of_dates() {
        let text = "{0=2200\n1=-7.2\n}";
        let (_, actual) = array(text).unwrap();
        assert_eq!(
            actual,
            vec![(0, Val::Integer(2200)), (1, Val::Decimal(-7.2))]
        );
    }
    #[test]
    fn indexed_value__simple_date__return_index_and_elem() {
        let text = "0=\"2200.01.01\"";
        let (_, (index, actual)) = indexed_value(text).unwrap();
        assert_eq!(index, 0);
        assert_eq!(
            actual,
            Val::Date(Date::from_calendar_date(2200, Month::January, 1).unwrap())
        );
    }

    #[test]
    fn dict__named_numbers__returns_array_of_dates() {
        let text = "{name=2200\nalias=-7.2\n}";
        let (_, actual) = dict(text).unwrap();

        assert_eq!(
            actual,
            vec![("name", Val::Integer(2200)), ("alias", Val::Decimal(-7.2))]
        );
    }

    #[test]
    fn key__lowercase_with_underscore__accepted() {
        let text = "name_with_underscore____d";
        let (_, actual) = key(text).unwrap();

        assert_eq!(actual, text);
    }

    #[test]
    fn root__assignment__returns_dict() {
        let text = "name=\"semantically_invalid\"\n";
        let (_, actual) = root(text).unwrap();
        if let Val::Dict(dict) = actual {
            assert!(dict.contains_key("name"));

            let first = dict.get("name").unwrap();

            assert_eq!(first, &vec![Val::String("semantically_invalid")]);
        } else {
            let mut string = String::from("Expected an Dict, but received a ");

            string.push_str(match actual {
                Val::Boolean(_) => "Boolean",
                Val::String(_) => "String",
                Val::Integer(_) => "Integer",
                Val::Decimal(_) => "Decimal",
                Val::Dict(_) => "Dict",
                Val::Array(_) => "Array",
                Val::Set(_) => "Set",
                Val::Date(_) => "Date",
            });
            panic!("{}", string);
        }
    }

    #[test]
    fn root__sample_meta_file_assignments__doesnt_break_please() {
        let path = PathBuf::from("/home/michael/Dev/stellarust/res/test_data/campaign_raw/unitednationsofearth_-15512622/autosave_2200.02.01/meta");

        let gamestate = fs::read_to_string(path).unwrap();

        let (str, actual) = root(gamestate.as_str()).unwrap();

        if let Val::Dict(d) = actual {
            println!("{:#?}", d);
        } else {
            panic!()
        }
    }

    // #[test]
    // fn root__sample_gamestate_file_assignments__doesnt_break_please() {
    //     let path = PathBuf::from("/home/michael/Dev/stellarust/res/test_data/campaign_raw/unitednationsofearth_-15512622/autosave_2200.02.01/gamestate");

    //     let gamestate = fs::read_to_string(path).unwrap();

    //     let (str, actual) = root(gamestate.as_str()).unwrap();

    //     if let Val::Dict(d) = actual {
    //         println!("{:#?}", d);
    //     } else {
    //         panic!()
    //     }
    // }
}
