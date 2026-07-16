use crate::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn epoch_milliseconds(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64() * 1_000.0,
        Err(error) => -(error.duration().as_secs_f64() * 1_000.0),
    }
}

pub struct JSDate<V: JSValueImpl> {
    inner: JSValue<V>,
}

impl<V: JSValueImpl> JSDate<V> {
    /// Create a new JSDate from epoch milliseconds
    pub fn new(ctx: &JSContext<V::Context>, epoch_ms: f64) -> Self {
        let value = V::create_date(ctx.as_ref(), epoch_ms);
        Self {
            inner: JSValue::from_raw(ctx, value),
        }
    }

    /// Create a JSDate for the current time
    pub fn now(ctx: &JSContext<V::Context>) -> Self {
        let now = epoch_milliseconds(SystemTime::now());
        Self::new(ctx, now)
    }

    /// Create a JSDate from SystemTime
    pub fn from_system_time(ctx: &JSContext<V::Context>, time: SystemTime) -> Self {
        let epoch_ms = epoch_milliseconds(time);
        Self::new(ctx, epoch_ms)
    }

    /// Get the epoch milliseconds by calling JavaScript getTime() method
    pub fn get_time(&self) -> JSResult<f64>
    where
        V: JSValueImpl + JSTypeOf + JSValueConversion + JSObjectOps,
    {
        // Convert to JSObject to access methods
        let date_obj = self.inner.clone().into_object().ok_or_else(|| {
            HostError::new(crate::error::E_TYPE, "Date is not an object").with_name("TypeError")
        })?;

        // Get the getTime method
        let get_time = date_obj.get::<_, JSFunc<V>>("getTime")?;

        // Call getTime() method with the date object as 'this'
        get_time.call::<_, f64>(Some(date_obj), ())
    }

    /// Convert to SystemTime
    pub fn to_system_time(&self) -> JSResult<SystemTime>
    where
        V: JSValueImpl + JSTypeOf + JSValueConversion + JSObjectOps,
    {
        let epoch_ms = self.get_time()?;
        if !epoch_ms.is_finite() {
            return Err(HostError::range_error(
                crate::error::E_OUT_OF_RANGE,
                "JavaScript Date does not contain a finite timestamp",
            )
            .into());
        }

        let duration =
            std::time::Duration::try_from_secs_f64(epoch_ms.abs() / 1_000.0).map_err(|_| {
                HostError::range_error(
                    crate::error::E_OUT_OF_RANGE,
                    "JavaScript Date is outside the SystemTime range",
                )
            })?;
        let time = if epoch_ms.is_sign_negative() {
            UNIX_EPOCH.checked_sub(duration)
        } else {
            UNIX_EPOCH.checked_add(duration)
        };
        time.ok_or_else(|| {
            HostError::range_error(
                crate::error::E_OUT_OF_RANGE,
                "JavaScript Date is outside the SystemTime range",
            )
            .into()
        })
    }

    /// Get the underlying JSValue
    pub fn as_js_value(&self) -> &JSValue<V> {
        &self.inner
    }

    /// Convert into the underlying JSValue
    pub fn into_js_value(self) -> JSValue<V> {
        self.inner
    }

    /// Convert into the underlying engine value
    pub fn into_value(self) -> V {
        self.inner.into_value()
    }

    /// Borrow the underlying engine value
    pub fn as_value(&self) -> &V {
        self.inner.as_value()
    }
}

impl<V: JSValueImpl> From<JSDate<V>> for JSValue<V> {
    fn from(date: JSDate<V>) -> Self {
        date.inner
    }
}

impl<V: JSValueImpl> FromJSValue<V> for JSDate<V>
where
    V: JSTypeOf,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if !value.is_date() {
            return Err(HostError::new(crate::error::E_TYPE, "Value is not a Date")
                .with_name("TypeError")
                .into());
        }
        Ok(JSDate { inner: value })
    }
}

impl<V: JSValueImpl> IntoJSValue<V> for JSDate<V> {
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.inner
    }
}

// Support for SystemTime conversion
impl<V: JSValueImpl> FromJSValue<V> for SystemTime
where
    V: JSTypeOf + JSValueConversion + JSObjectOps,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let js_date = JSDate::from_js_value(ctx, value)?;
        js_date.to_system_time()
    }
}

impl<V: JSValueImpl> IntoJSValue<V> for SystemTime {
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        let epoch_ms = epoch_milliseconds(self);
        JSValue::from_raw(ctx, V::create_date(ctx.as_ref(), epoch_ms))
    }
}
