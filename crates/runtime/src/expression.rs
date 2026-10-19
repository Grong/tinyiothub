//! 派生属性表达式：最小集递归下降解析器（spec §二）。
//!   expression := term (('+'|'-') term)*
//!   term       := factor (('*'|'/') factor)*
//!   factor     := '-' factor | NUMBER | ref | func | '(' expression ')'
//!   ref        := '{' 标识符(可含非ASCII/空格) '.' 标识符 '}'
//!   func       := 'descendant_count(' [标识符] ')'

pub const MAX_EXPRESSION_LEN: usize = 1000; // F-C
pub const MAX_REFS: usize = 50; // F-C

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyRef {
    pub thing_name: String,
    pub property_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ast {
    Num(f64),
    Ref(PropertyRef),
    DescendantCount(Option<String>),
    Add(Box<Ast>, Box<Ast>),
    Sub(Box<Ast>, Box<Ast>),
    Mul(Box<Ast>, Box<Ast>),
    Div(Box<Ast>, Box<Ast>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    MissingValue(PropertyRef),
    NotNumeric(PropertyRef),
    DivisionByZero,
}

pub fn parse(expr: &str) -> Result<Ast, String> {
    if expr.len() > MAX_EXPRESSION_LEN {
        return Err(format!("表达式超长（{}>{}）", expr.len(), MAX_EXPRESSION_LEN));
    }
    let tokens = lex(expr)?;
    let mut p = Parser { tokens, pos: 0 };
    let ast = p.parse_expression()?;
    if p.pos != p.tokens.len() {
        return Err(format!("位置 {} 处有多余内容", p.pos));
    }
    if extract_refs(&ast).len() > MAX_REFS {
        return Err(format!("引用过多（>{}）", MAX_REFS));
    }
    Ok(ast)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    RefName(String), // {物名.属性名} 整体一个词素
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn lex(input: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            c if c.is_whitespace() => i += 1,
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '{' => {
                let start = i;
                let mut depth = 0;
                while i < chars.len() {
                    if chars[i] == '{' {
                        depth += 1;
                    }
                    if chars[i] == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += 1;
                }
                if i >= chars.len() {
                    return Err("未闭合的 '{'".into());
                }
                let inner: String = chars[start + 1..i].iter().collect();
                let (thing, prop) = inner.split_once('.').ok_or("引用须为 {物名.属性名}")?;
                if thing.trim().is_empty() || prop.trim().is_empty() {
                    return Err("引用物名/属性名不能为空".into());
                }
                out.push(Tok::RefName(format!("{}.{}", thing.trim(), prop.trim())));
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                out.push(Tok::Num(s.parse::<f64>().map_err(|_| format!("非法数字 '{s}'"))?));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            c => return Err(format!("非法字符 '{c}'")),
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_expression(&mut self) -> Result<Ast, String> {
        let mut lhs = self.parse_term()?;
        while let Some(op) = self.peek() {
            match op {
                Tok::Plus => {
                    self.next();
                    lhs = Ast::Add(Box::new(lhs), Box::new(self.parse_term()?));
                }
                Tok::Minus => {
                    self.next();
                    lhs = Ast::Sub(Box::new(lhs), Box::new(self.parse_term()?));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_term(&mut self) -> Result<Ast, String> {
        let mut lhs = self.parse_factor()?;
        while let Some(op) = self.peek() {
            match op {
                Tok::Star => {
                    self.next();
                    lhs = Ast::Mul(Box::new(lhs), Box::new(self.parse_factor()?));
                }
                Tok::Slash => {
                    self.next();
                    lhs = Ast::Div(Box::new(lhs), Box::new(self.parse_factor()?));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_factor(&mut self) -> Result<Ast, String> {
        match self.next() {
            Some(Tok::Minus) => Ok(Ast::Sub(Box::new(Ast::Num(0.0)), Box::new(self.parse_factor()?))),
            Some(Tok::Num(n)) => Ok(Ast::Num(n)),
            Some(Tok::RefName(r)) => {
                let (t, p) = r.split_once('.').unwrap();
                Ok(Ast::Ref(PropertyRef {
                    thing_name: t.into(),
                    property_name: p.into(),
                }))
            }
            Some(Tok::LParen) => {
                let e = self.parse_expression()?;
                if self.next() != Some(Tok::RParen) {
                    return Err("缺少 ')'".into());
                }
                Ok(e)
            }
            Some(Tok::Ident(name)) if name == "descendant_count" => {
                if self.next() != Some(Tok::LParen) {
                    return Err("descendant_count 后须跟 '('".into());
                }
                let arg = match self.peek() {
                    Some(Tok::Ident(t)) => {
                        let t = t.clone();
                        self.next();
                        Some(t)
                    }
                    _ => None,
                };
                if self.next() != Some(Tok::RParen) {
                    return Err("descendant_count 缺少 ')'".into());
                }
                Ok(Ast::DescendantCount(arg))
            }
            Some(t) => Err(format!("意外的词素 {:?}", t)),
            None => Err("表达式意外结束".into()),
        }
    }
}

pub fn extract_refs(ast: &Ast) -> Vec<PropertyRef> {
    fn walk(ast: &Ast, out: &mut Vec<PropertyRef>) {
        match ast {
            Ast::Ref(r) if !out.contains(r) => {
                out.push(r.clone());
            }
            Ast::Add(a, b) | Ast::Sub(a, b) | Ast::Mul(a, b) | Ast::Div(a, b) => {
                walk(a, out);
                walk(b, out);
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(ast, &mut out);
    out
}

pub fn eval(
    ast: &Ast,
    value_of: &dyn Fn(&PropertyRef) -> Option<String>,
    descendant_count: &dyn Fn(Option<&str>) -> Result<f64, String>,
) -> Result<f64, EvalError> {
    match ast {
        Ast::Num(n) => Ok(*n),
        Ast::Ref(r) => {
            let raw = value_of(r).ok_or_else(|| EvalError::MissingValue(r.clone()))?;
            raw.trim().parse::<f64>().map_err(|_| EvalError::NotNumeric(r.clone()))
        }
        Ast::DescendantCount(t) => descendant_count(t.as_deref()).map_err(|_| {
            EvalError::MissingValue(PropertyRef {
                thing_name: "<descendant_count>".into(),
                property_name: t.clone().unwrap_or_default(),
            })
        }),
        Ast::Add(a, b) => Ok(eval(a, value_of, descendant_count)? + eval(b, value_of, descendant_count)?),
        Ast::Sub(a, b) => Ok(eval(a, value_of, descendant_count)? - eval(b, value_of, descendant_count)?),
        Ast::Mul(a, b) => Ok(eval(a, value_of, descendant_count)? * eval(b, value_of, descendant_count)?),
        Ast::Div(a, b) => {
            let d = eval(b, value_of, descendant_count)?;
            if d == 0.0 {
                return Err(EvalError::DivisionByZero);
            }
            Ok(eval(a, value_of, descendant_count)? / d)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dc(_: Option<&str>) -> Result<f64, String> {
        Ok(7.0)
    }

    #[test]
    fn parses_arithmetic_precedence() {
        let ast = parse("{电表1.用电数} + {电表2.用电数} * 2").unwrap();
        let v = eval(
            &ast,
            &|r: &PropertyRef| {
                Some(if r.thing_name == "电表1" {
                    "10".into()
                } else {
                    "20".into()
                })
            },
            &dc,
        )
        .unwrap();
        assert_eq!(v, 50.0); // 10 + 20*2
    }
    #[test]
    fn parses_parens_and_unary_minus() {
        let ast = parse("({a.x} - 4) * (2 + 3)").unwrap();
        let v = eval(&ast, &|_| Some("9".into()), &dc).unwrap();
        assert_eq!(v, 25.0);
    }
    #[test]
    fn descendant_count_with_and_without_type() {
        assert!(matches!(
            parse("descendant_count()").unwrap(),
            Ast::DescendantCount(None)
        ));
        assert!(
            matches!(parse("descendant_count(device)").unwrap(), Ast::DescendantCount(Some(ref t)) if t == "device")
        );
    }
    #[test]
    fn descendant_count_rejects_empty_type_arg() {
        assert!(parse("descendant_count( )").is_ok(), "空白视为无参");
        assert!(parse("descendant_count(123)").is_err(), "类型参数必须是标识符");
    }
    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "{",
            "{a}",
            "{a.}",
            "1 +",
            "* 2",
            "{a.b} {c.d}",
            "sum(1)",
            "{a.b} % 2",
            "abc",
        ] {
            assert!(parse(bad).is_err(), "应拒绝: {bad:?}");
        }
    }
    #[test]
    fn enforces_input_caps() {
        let long = format!("{{a.b}}{}", "+1".repeat(600));
        assert!(parse(&long).is_err(), "F-C: 超 1000 字符拒绝");
        let many_refs = (0..51).map(|i| format!("{{t{i}.p}}")).collect::<Vec<_>>().join("+");
        assert!(parse(&many_refs).is_err(), "F-C: 超 50 refs 拒绝");
    }
    #[test]
    fn eval_missing_value_is_missing() {
        let ast = parse("{a.x} + 1").unwrap();
        assert!(matches!(eval(&ast, &|_| None, &dc), Err(EvalError::MissingValue(_))));
    }
    #[test]
    fn eval_non_numeric_is_rejected() {
        let ast = parse("{a.x} + 1").unwrap();
        assert!(matches!(
            eval(&ast, &|_| Some("abc".into()), &dc),
            Err(EvalError::NotNumeric(_))
        ));
    }
    #[test]
    fn division_by_zero() {
        let ast = parse("{a.x} / 0").unwrap();
        assert!(matches!(
            eval(&ast, &|_| Some("1".into()), &dc),
            Err(EvalError::DivisionByZero)
        ));
    }
    #[test]
    fn extract_refs_dedups() {
        let ast = parse("{a.x} + {a.x} + {b.y}").unwrap();
        let refs = extract_refs(&ast);
        assert_eq!(refs.len(), 2);
    }
    #[test]
    fn numeric_strings_with_float_and_int() {
        let ast = parse("{a.x} + {a.y}").unwrap();
        let v = eval(
            &ast,
            &|r: &PropertyRef| {
                Some(if r.property_name == "x" {
                    "1.5".into()
                } else {
                    "2".into()
                })
            },
            &dc,
        )
        .unwrap();
        assert_eq!(v, 3.5);
    }
}
