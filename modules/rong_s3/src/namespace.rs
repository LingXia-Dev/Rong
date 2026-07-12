use rong::{HostError, JSContext, JSResult};
use std::borrow::Cow;
use std::rc::Rc;

type NamespaceResolver = Rc<dyn Fn(&JSContext) -> JSResult<String>>;

#[derive(Clone, Default)]
pub(crate) enum S3Namespace {
    #[default]
    None,
    Static(String),
    Dynamic(NamespaceResolver),
}

impl S3Namespace {
    pub(crate) fn fixed(prefix: impl Into<String>) -> JSResult<Self> {
        let prefix = prefix.into();
        Self::validate(&prefix)?;
        Ok(Self::Static(prefix))
    }

    pub(crate) fn dynamic<F>(resolver: F) -> Self
    where
        F: Fn(&JSContext) -> JSResult<String> + 'static,
    {
        Self::Dynamic(Rc::new(resolver))
    }

    pub(crate) fn resolve<'a>(&'a self, ctx: &JSContext) -> JSResult<Option<Cow<'a, str>>> {
        match self {
            Self::None => Ok(None),
            Self::Static(prefix) => Ok(Some(Cow::Borrowed(prefix.as_str()))),
            Self::Dynamic(resolver) => {
                let prefix = resolver(ctx)?;
                Self::validate(&prefix)?;
                Ok(Some(Cow::Owned(prefix)))
            }
        }
    }

    pub(crate) fn apply(prefix: Option<&str>, path: &str) -> String {
        match prefix {
            Some(prefix) => format!("{prefix}{path}"),
            None => path.to_owned(),
        }
    }

    pub(crate) fn strip(prefix: Option<&str>, key: &str) -> JSResult<String> {
        match prefix {
            Some(prefix) => key.strip_prefix(prefix).map(str::to_owned).ok_or_else(|| {
                HostError::new(
                    "E_INVALID_STATE",
                    "S3 returned an object outside the resolved namespace",
                )
                .with_name("S3Error")
                .into()
            }),
            None => Ok(key.to_owned()),
        }
    }

    fn validate(prefix: &str) -> JSResult<()> {
        if prefix.is_empty() {
            return Err(
                HostError::new("E_INVALID_ARG", "S3 namespace must not be empty")
                    .with_name("TypeError")
                    .into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_keys_inside_the_namespace() {
        assert_eq!(
            S3Namespace::strip(Some("scope/"), "scope/key.txt").expect("namespaced key"),
            "key.txt"
        );
        assert!(S3Namespace::strip(Some("scope/"), "other/key.txt").is_err());
        assert_eq!(
            S3Namespace::strip(None, "raw/key.txt").expect("raw key"),
            "raw/key.txt"
        );
    }
}
