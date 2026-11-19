bool _QJS_SetCFuncMagic(JSValue func_obj, int magic){
    JSObject *p;
    if (JS_VALUE_GET_TAG(func_obj) != JS_TAG_OBJECT)
        return false;

    p = JS_VALUE_GET_OBJ(func_obj);
    p->u.cfunc.magic = magic;
    return true;
}

int _QJS_GetCFuncMagic(JSValue func_obj){
    JSObject *p;
    if (JS_VALUE_GET_TAG(func_obj) != JS_TAG_OBJECT)
        return -1;

    p = JS_VALUE_GET_OBJ(func_obj);
    return p->u.cfunc.magic;
}

int QJS_GetRefCount(JSValue v)
{
    int count=-1;
    if (JS_VALUE_HAS_REF_COUNT(v)) {
        // The count lives in the allocator block header since QuickJS-NG 0.18.
        count = JS_REF_COUNT(JS_VALUE_GET_PTR(v));
    }
    return count;
}

void QJS_Free(JSContext *ctx, void *ptr)
{
    if (ptr) {
        js_free(ctx, ptr);
    }
}

int32_t QJS_ValueIdentTag(JSValue v)
{
    return JS_VALUE_GET_NORM_TAG(v);
}

uint64_t QJS_ValueIdentPayload(JSValue v)
{
    int32_t tag = JS_VALUE_GET_NORM_TAG(v);

    if (tag == JS_TAG_FLOAT64) {
        union {
            double d;
            uint64_t u;
        } bits;
        bits.d = JS_VALUE_GET_FLOAT64(v);
        return bits.u;
    }

    if (tag < 0) {
        return (uint64_t)(uintptr_t)JS_VALUE_GET_PTR(v);
    }

    if (tag == JS_TAG_SHORT_BIG_INT) {
        return (uint32_t)JS_VALUE_GET_SHORT_BIG_INT(v);
    }

    return (uint32_t)JS_VALUE_GET_INT(v);
}
