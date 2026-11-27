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

#ifdef CONFIG_RONG_JIT

void QJS_SetJitHooks(
    JSRuntime *rt,
    void *opaque,
    QJSJitCallHook call_hook,
    QJSJitFreeBytecodeHook free_bc_hook,
    QJSJitFreeRuntimeHook free_rt_hook
)
{
    rt->jit_opaque = opaque;
    rt->jit_call = call_hook;
    rt->jit_free_bytecode = free_bc_hook;
    rt->jit_free_runtime = free_rt_hook;
    rt->jit_enabled = (call_hook != NULL) ? 1 : 0;
}

void *QJS_GetJitOpaque(JSRuntime *rt)
{
    return rt->jit_opaque;
}

void QJS_SetJitEnabled(JSRuntime *rt, int enabled)
{
    rt->jit_enabled = enabled ? 1 : 0;
}

uint64_t QJS_JitGetDirectCalls(JSRuntime *rt)
{
    return rt->jit_direct_calls;
}

uint64_t *QJS_JitDirectCallsPtr(JSRuntime *rt)
{
    return &rt->jit_direct_calls;
}

const uint8_t *QJS_JitEnabledPtr(JSRuntime *rt)
{
    return &rt->jit_enabled;
}

const uintptr_t *QJS_JitStackLimitPtr(JSRuntime *rt)
{
    return &rt->stack_limit;
}

void QJS_JitGetBytecodeInfo(const void *bc, QJSJitBytecodeInfo *out)
{
    const JSFunctionBytecode *b = (const JSFunctionBytecode *)bc;
    out->buf = b->byte_code_buf;
    out->len = (uint32_t)b->byte_code_len;
    out->arg_count = b->arg_count;
    out->var_count = b->var_count;
    out->stack_size = b->stack_size;
    out->var_ref_count = b->var_ref_count;
    out->closure_var_count = b->closure_var_count;
    out->is_strict_mode = b->is_strict_mode;
    out->func_kind = b->func_kind;
    out->cpool_count = b->cpool_count;
    out->cpool = b->cpool;
    out->realm = b->realm;
}

void *QJS_JitGetBytecodeData(const void *bc)
{
    const JSFunctionBytecode *b = (const JSFunctionBytecode *)bc;
    return b->jit_data;
}

void QJS_JitSetBytecodeData(void *bc, void *data)
{
    JSFunctionBytecode *b = (JSFunctionBytecode *)bc;
    b->jit_data = data;
}

void QJS_JitSetBytecodeEntry(void *bc, QJSJitNativeFn fn)
{
    JSFunctionBytecode *b = (JSFunctionBytecode *)bc;
    b->jit_entry = fn;
}

uint32_t QJS_JitIncCallCount(void *bc)
{
    JSFunctionBytecode *b = (JSFunctionBytecode *)bc;
    return ++b->jit_call_count;
}

uint32_t QJS_JitValueSize(void)
{
    return (uint32_t)sizeof(JSValue);
}

uint32_t QJS_JitStackFrameSize(void)
{
    return (uint32_t)sizeof(JSStackFrame);
}

int QJS_JitCheckValueLayout(void)
{
#ifdef JS_NAN_BOXING
    return 0;
#else
    return sizeof(JSValue) == 16 &&
           offsetof(JSValue, u) == 0 &&
           offsetof(JSValue, tag) == 8 &&
           offsetof(JSStackFrame, prev_frame) == 0 &&
           sizeof(JSStackFrame) >= 8 &&
           (sizeof(JSStackFrame) % 8) == 0;
#endif
}

void **QJS_JitCurrentStackFramePtr(JSRuntime *rt)
{
    return (void **)&rt->current_stack_frame;
}

void QJS_JitOnException(JSContext *ctx)
{
    JSRuntime *rt = ctx->rt;
    if (needs_backtrace(rt->current_exception) || JS_IsUndefined(ctx->error_back_trace)) {
        build_backtrace(ctx, rt->current_exception, JS_UNDEFINED, NULL, 0, 0, 0);
    }
}

void QJS_JitInitUndefined(JSValue *slots, uint32_t count)
{
    for (uint32_t i = 0; i < count; i++) {
        slots[i] = JS_UNDEFINED;
    }
}

void QJS_JitFreeSlots(JSContext *ctx, const JSValue *slots, uint32_t count)
{
    for (uint32_t i = 0; i < count; i++) {
        JS_FreeValue(ctx, slots[i]);
    }
}

static inline int jit_invoke_impl(
    JSContext *ctx,
    QJSJitNativeFn fn,
    const JSFunctionBytecode *b,
    const JSValue *func_obj,
    const JSValue *this_obj,
    int argc,
    const JSValue *argv,
    JSValue *ret_out
)
{
    JSRuntime *rt = ctx->rt;
    JSStackFrame sf;
    int rc;

    sf.prev_frame = rt->current_stack_frame;
    sf.cur_func = *func_obj;
    sf.arg_buf = NULL;
    sf.var_buf = NULL;
    sf.var_refs = NULL;
    sf.cur_pc = b->byte_code_buf + 1;
    sf.var_ref_count = 0;
    sf.arg_count = 0;
    sf.is_strict_mode = b->is_strict_mode;
    sf.is_constructor = false;
    sf.cur_sp = NULL;
    sf.cur_gc_obj = NULL;
    rt->current_stack_frame = &sf;

    rc = fn(ctx, func_obj, this_obj, argc, argv, ret_out);

    if (unlikely(rc == 0 && JS_IsException(*ret_out))) {
        QJS_JitOnException(ctx);
    }
    rt->current_stack_frame = sf.prev_frame;
    return rc;
}

int QJS_JitInvoke(
    JSContext *ctx,
    QJSJitNativeFn fn,
    const void *bc,
    const JSValue *func_obj,
    const JSValue *this_obj,
    int argc,
    const JSValue *argv,
    JSValue *ret_out
)
{
    return jit_invoke_impl(
        ctx,
        fn,
        (const JSFunctionBytecode *)bc,
        func_obj,
        this_obj,
        argc,
        argv,
        ret_out
    );
}

int QJS_JitPollInterrupts(JSContext *ctx)
{
    return js_poll_interrupts(ctx);
}

int QJS_JitCheckStack(JSContext *ctx, size_t frame_bytes)
{
    if (js_check_stack_overflow(ctx->rt, frame_bytes)) {
        JS_ThrowStackOverflow(ctx);
        return 1;
    }
    return 0;
}

void QJS_JitDupValue(JSContext *ctx, const JSValue *v)
{
    (void)ctx;
    JS_DupValue(ctx, *v);
}

void QJS_JitFreeValue(JSContext *ctx, const JSValue *v)
{
    JS_FreeValue(ctx, *v);
}

int QJS_JitToBool(JSContext *ctx, const JSValue *v)
{
    return JS_ToBool(ctx, *v);
}

int QJS_JitBoxThis(JSContext *ctx, const JSValue *this_obj, JSValue *out)
{
    JSValue v = *this_obj;
    uint32_t tag = JS_VALUE_GET_TAG(v);
    JSValue val;
    if (tag == JS_TAG_OBJECT) {
        val = JS_DupValue(ctx, v);
    } else if (tag == JS_TAG_NULL || tag == JS_TAG_UNDEFINED) {
        val = JS_DupValue(ctx, ctx->global_obj);
    } else {
        val = JS_ToObject(ctx, v);
        if (JS_IsException(val)) {
            *out = JS_EXCEPTION;
            return 1;
        }
    }
    *out = val;
    return 0;
}

void QJS_JitThrowUninitialized(JSContext *ctx, uint32_t idx)
{
    JSStackFrame *sf = ctx->rt->current_stack_frame;
    JSFunctionBytecode *b = sf ? JS_GetFunctionBytecode(sf->cur_func) : NULL;
    if (b) {
        JS_ThrowReferenceErrorUninitialized2(ctx, b, (int)idx, false);
    } else {
        JS_ThrowReferenceErrorUninitialized(ctx, JS_ATOM_NULL);
    }
}

int QJS_JitCall(
    JSContext *ctx,
    const JSValue *func_obj,
    const JSValue *this_obj,
    int argc,
    const JSValue *argv,
    JSValue *ret_out
)
{
    /* Fast path: direct JIT-to-JIT dispatch without JS_CallInternal or Rust hook overhead. */
    if (likely(JS_VALUE_GET_TAG(*func_obj) == JS_TAG_OBJECT)) {
        JSObject *p = JS_VALUE_GET_OBJ(*func_obj);
        if (likely(p->class_id == JS_CLASS_BYTECODE_FUNCTION)) {
            JSFunctionBytecode *b = p->u.func.function_bytecode;
            QJSJitNativeFn fn = b->jit_entry;
            JSRuntime *rt = ctx->rt;
            if (likely(fn != NULL && rt->jit_enabled)) {
                if (unlikely((++rt->jit_direct_calls & 1023) == 0) && js_poll_interrupts(ctx)) {
                    *ret_out = JS_EXCEPTION;
                    return 1;
                }
                JSContext *exec_ctx = b->realm ? b->realm : ctx;
                int rc = jit_invoke_impl(exec_ctx, fn, b, func_obj, this_obj, argc, argv, ret_out);
                if (likely(rc == 0)) {
                    return JS_IsException(*ret_out) ? 1 : 0;
                }
            }
        }
    }

    JSValue res = JS_CallInternal(
        ctx,
        *func_obj,
        *this_obj,
        JS_UNDEFINED,
        argc,
        (JSValue *)argv,
        JS_CALL_FLAG_COPY_ARGV
    );
    *ret_out = res;
    return JS_IsException(res) ? 1 : 0;
}

int QJS_JitBinarySlow(
    JSContext *ctx,
    int opcode,
    const JSValue *a,
    const JSValue *b,
    JSValue *out
)
{
    JSValue sp[2];
    int ret;
    sp[0] = JS_DupValue(ctx, *a);
    sp[1] = JS_DupValue(ctx, *b);
    switch ((OPCodeEnum)opcode) {
    case OP_add:
        ret = js_add_slow(ctx, sp + 2);
        break;
    case OP_sub:
    case OP_mul:
    case OP_div:
    case OP_mod:
    case OP_pow:
        ret = js_binary_arith_slow(ctx, sp + 2, (OPCodeEnum)opcode);
        break;
    case OP_shr:
        ret = js_shr_slow(ctx, sp + 2);
        break;
    case OP_shl:
    case OP_sar:
    case OP_and:
    case OP_or:
    case OP_xor:
        ret = js_binary_logic_slow(ctx, sp + 2, (OPCodeEnum)opcode);
        break;
    case OP_lt:
    case OP_lte:
    case OP_gt:
    case OP_gte:
        ret = js_relational_slow(ctx, sp + 2, (OPCodeEnum)opcode);
        break;
    case OP_eq:
        ret = js_eq_slow(ctx, sp + 2, false);
        break;
    case OP_neq:
        ret = js_eq_slow(ctx, sp + 2, true);
        break;
    case OP_strict_eq:
        ret = js_strict_eq_slow(ctx, sp + 2, false);
        break;
    case OP_strict_neq:
        ret = js_strict_eq_slow(ctx, sp + 2, true);
        break;
    default:
        JS_FreeValue(ctx, sp[0]);
        JS_FreeValue(ctx, sp[1]);
        JS_ThrowInternalError(ctx, "unsupported JIT binary slow opcode %d", opcode);
        *out = JS_EXCEPTION;
        return 1;
    }
    if (ret) {
        *out = JS_EXCEPTION;
        return 1;
    }
    *out = sp[0];
    return 0;
}

int QJS_JitUnarySlow(
    JSContext *ctx,
    int opcode,
    const JSValue *a,
    JSValue *out
)
{
    JSValue sp[1];
    int ret;
    sp[0] = JS_DupValue(ctx, *a);
    switch ((OPCodeEnum)opcode) {
    case OP_neg:
    case OP_plus:
    case OP_inc:
    case OP_dec:
        ret = js_unary_arith_slow(ctx, sp + 1, (OPCodeEnum)opcode);
        break;
    case OP_not:
        ret = js_not_slow(ctx, sp + 1);
        break;
    default:
        JS_FreeValue(ctx, sp[0]);
        JS_ThrowInternalError(ctx, "unsupported JIT unary slow opcode %d", opcode);
        *out = JS_EXCEPTION;
        return 1;
    }
    if (ret) {
        *out = JS_EXCEPTION;
        return 1;
    }
    *out = sp[0];
    return 0;
}

int QJS_JitPostIncSlow(
    JSContext *ctx,
    int opcode,
    const JSValue *a,
    JSValue *old_out,
    JSValue *new_out
)
{
    JSValue sp[2];
    int ret;
    sp[0] = JS_DupValue(ctx, *a);
    sp[1] = JS_UNDEFINED;
    ret = js_post_inc_slow(ctx, sp + 1, (OPCodeEnum)opcode);
    if (ret) {
        JS_FreeValue(ctx, sp[0]);
        *old_out = JS_EXCEPTION;
        *new_out = JS_EXCEPTION;
        return 1;
    }
    *old_out = sp[0];
    *new_out = sp[1];
    return 0;
}

int QJS_JitGetVar(JSContext *ctx, uint32_t atom, JSValue *out)
{
    JSValue val = JS_GetGlobalVar(ctx, (JSAtom)atom, true);
    *out = val;
    return JS_IsException(val) ? 1 : 0;
}

int QJS_JitGetVarUndef(JSContext *ctx, uint32_t atom, JSValue *out)
{
    JSValue val = JS_GetGlobalVar(ctx, (JSAtom)atom, false);
    *out = val;
    return JS_IsException(val) ? 1 : 0;
}

int QJS_JitPutVar(JSContext *ctx, uint32_t atom, const JSValue *val)
{
    int ret = JS_SetGlobalVar(ctx, (JSAtom)atom, JS_DupValue(ctx, *val), 0);
    return ret < 0 ? 1 : 0;
}

int QJS_JitGetField(JSContext *ctx, const JSValue *obj, uint32_t atom, JSValue *out)
{
    JSValue val = JS_GetProperty(ctx, *obj, (JSAtom)atom);
    *out = val;
    return JS_IsException(val) ? 1 : 0;
}

int QJS_JitPutField(JSContext *ctx, const JSValue *obj, uint32_t atom, const JSValue *val)
{
    int ret = JS_SetPropertyInternal(
        ctx,
        *obj,
        (JSAtom)atom,
        JS_DupValue(ctx, *val),
        JS_PROP_THROW_STRICT
    );
    return ret < 0 ? 1 : 0;
}

int QJS_JitGetArrayEl(JSContext *ctx, const JSValue *obj, const JSValue *idx, JSValue *out)
{
    JSValue val;
    if (likely(JS_VALUE_GET_TAG(*obj) == JS_TAG_OBJECT &&
               JS_VALUE_GET_TAG(*idx) == JS_TAG_INT)) {
        JSObject *p = JS_VALUE_GET_OBJ(*obj);
        int32_t i = JS_VALUE_GET_INT(*idx);
        if (likely(i >= 0)) {
            if (likely(p->class_id == JS_CLASS_ARRAY && (uint32_t)i < (uint32_t)p->u.array.count)) {
                *out = js_dup(p->u.array.u.values[i]);
                return 0;
            }
            if (js_get_fast_array_element(ctx, p, (uint32_t)i, &val)) {
                *out = val;
                return JS_IsException(val) ? 1 : 0;
            }
        }
    }
    val = JS_GetPropertyValue(ctx, *obj, JS_DupValue(ctx, *idx));
    *out = val;
    return JS_IsException(val) ? 1 : 0;
}

int QJS_JitPutArrayEl(JSContext *ctx, const JSValue *obj, const JSValue *idx, const JSValue *val)
{
    int ret;
    if (likely(JS_VALUE_GET_TAG(*obj) == JS_TAG_OBJECT &&
               JS_VALUE_GET_TAG(*idx) == JS_TAG_INT)) {
        JSObject *p = JS_VALUE_GET_OBJ(*obj);
        int32_t i = JS_VALUE_GET_INT(*idx);
        if (likely(i >= 0 && p->class_id == JS_CLASS_ARRAY && (uint32_t)i < (uint32_t)p->u.array.count)) {
            set_value(ctx, &p->u.array.u.values[i], JS_DupValue(ctx, *val));
            return 0;
        }
    }
    ret = JS_SetPropertyValue(
        ctx,
        *obj,
        JS_DupValue(ctx, *idx),
        JS_DupValue(ctx, *val),
        JS_PROP_THROW_STRICT
    );
    return ret < 0 ? 1 : 0;
}

int QJS_JitGetLength(JSContext *ctx, const JSValue *obj, JSValue *out)
{
    JSValue val = JS_GetProperty(ctx, *obj, JS_ATOM_length);
    *out = val;
    return JS_IsException(val) ? 1 : 0;
}

int QJS_JitGetClosureVar(JSContext *ctx, const JSValue *func_obj, uint32_t idx, JSValue *out)
{
    JSObject *p = JS_VALUE_GET_OBJ(*func_obj);
    JSValue val = *p->u.func.var_refs[idx]->pvalue;
    if (unlikely(JS_IsUninitialized(val))) {
        JSFunctionBytecode *b = p->u.func.function_bytecode;
        JSAtom name = (b->closure_var && idx < b->closure_var_count)
            ? b->closure_var[idx].var_name
            : JS_ATOM_NULL;
        JS_ThrowReferenceErrorUninitialized(ctx, name);
        *out = JS_EXCEPTION;
        return 1;
    }
    *out = JS_DupValue(ctx, val);
    return 0;
}

int QJS_JitPutClosureVar(JSContext *ctx, const JSValue *func_obj, uint32_t idx, const JSValue *val)
{
    JSObject *p = JS_VALUE_GET_OBJ(*func_obj);
    JSValue *slot = p->u.func.var_refs[idx]->pvalue;
    set_value(ctx, slot, JS_DupValue(ctx, *val));
    return 0;
}

#endif /* CONFIG_RONG_JIT */
