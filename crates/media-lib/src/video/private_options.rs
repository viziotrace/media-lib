use ffmpeg_next::ffi;
use std::ffi::CStr;
use std::fmt;

pub enum OptionValue {
    Int {
        default: i64,
        min: Option<i64>,
        max: Option<i64>,
    },
    Flags {
        default: i64,
    },
    String {
        default: String,
    },
    Other,
}

impl fmt::Debug for OptionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptionValue::Int { default, min, max } => {
                write!(f, "Int({}", default)?;
                if let Some(min) = min {
                    write!(f, ", min={}", min)?;
                }
                if let Some(max) = max {
                    write!(f, ", max={}", max)?;
                }
                write!(f, ")")
            }
            OptionValue::Flags { default } => {
                write!(f, "Flags(0b{:b})", default)
            }
            OptionValue::String { default } => {
                write!(f, "String(\"{}\")", default)
            }
            OptionValue::Other => write!(f, "Other(unknown type)"),
        }
    }
}

pub struct CodecOption {
    pub name: String,
    pub help: String,
    pub value_type: OptionValue,
}

impl fmt::Debug for CodecOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "╭─ {} ─", self.name)?;
        writeln!(f, "│ Type: {:?}", self.value_type)?;
        let help_lines: Vec<_> = self.help.split('\n').collect();
        for line in help_lines {
            writeln!(f, "│ Help: {}", line)?;
        }
        write!(f, "╰{}", "─".repeat(40))
    }
}

pub struct CodecPrivateOptions {
    pub codec_name: String,
    pub options: Vec<CodecOption>,
}

impl fmt::Debug for CodecPrivateOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "┏━━ Codec: {} ━━", self.codec_name)?;
        writeln!(f, "┣━━ Options: {}", self.options.len())?;
        for option in &self.options {
            writeln!(f, "┃")?;
            writeln!(f, "┃ {:?}", option)?;
        }
        write!(f, "┗{}", "━".repeat(40))
    }
}

pub fn get_codec_private_options(codec: &ffmpeg_next::Codec) -> CodecPrivateOptions {
    let mut options = Vec::new();

    unsafe {
        let class = (*codec.as_ptr()).priv_class;
        if class.is_null() {
            return CodecPrivateOptions {
                codec_name: codec.name().to_string(),
                options,
            };
        }

        let mut opt = std::ptr::null();

        loop {
            opt = ffi::av_opt_next(&class as *const _ as *const _, opt);
            if opt.is_null() {
                break;
            }

            let name = CStr::from_ptr((*opt).name).to_string_lossy().to_string();
            let help = if (*opt).help.is_null() {
                "No help available".to_string()
            } else {
                CStr::from_ptr((*opt).help).to_string_lossy().to_string()
            };

            let value_type = match (*opt).type_ {
                ffi::AVOptionType::AV_OPT_TYPE_INT => {
                    let default_val = (*opt).default_val.i64_;
                    let min = (*opt).min as i64;
                    let max = (*opt).max as i64;
                    OptionValue::Int {
                        default: default_val,
                        min: if min != i64::MIN { Some(min) } else { None },
                        max: if max != i64::MAX { Some(max) } else { None },
                    }
                }
                ffi::AVOptionType::AV_OPT_TYPE_FLAGS => {
                    let default_val = (*opt).default_val.i64_;
                    OptionValue::Flags {
                        default: default_val,
                    }
                }
                ffi::AVOptionType::AV_OPT_TYPE_STRING => {
                    let default_str = if (*opt).default_val.str_.is_null() {
                        "null".to_string()
                    } else {
                        CStr::from_ptr((*opt).default_val.str_)
                            .to_string_lossy()
                            .to_string()
                    };
                    OptionValue::String {
                        default: default_str,
                    }
                }
                _ => OptionValue::Other,
            };

            options.push(CodecOption {
                name,
                help,
                value_type,
            });
        }
    }

    CodecPrivateOptions {
        codec_name: codec.name().to_string(),
        options,
    }
}
