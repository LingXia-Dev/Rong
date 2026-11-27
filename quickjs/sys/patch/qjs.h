#ifndef QJS_H
#define QJS_H

#include "quickjs.h"

JSValue QJS_NewBool(JSContext *ctx, bool val);

JSValue QJS_NewInt32(JSContext *ctx, int32_t val);
int QJS_ToUint32(JSContext *ctx, uint32_t *pres, JSValue val);

JSValue QJS_NewFloat64(JSContext *ctx, double val);

JSValue QJS_NewUint32(JSContext *ctx, uint32_t val);
JSValue QJS_NewUndefined(JSContext *ctx);
JSValue QJS_NewNull(JSContext *ctx);
bool QJS_IsNumber(JSContext *ctx, JSValue v);
bool QJS_IsBigInt(JSContext *ctx, JSValue v);
bool QJS_IsBool(JSContext *ctx, JSValue v);
bool QJS_IsUndefined(JSContext *ctx, JSValue v);
bool QJS_IsException(JSContext *ctx, JSValue v);
bool QJS_IsNull(JSContext *ctx, JSValue v);
bool QJS_IsString(JSContext *ctx, JSValue v);
bool QJS_IsSymbol(JSContext *ctx, JSValue v);
bool QJS_IsObject(JSContext *ctx, JSValue v);

/* Debug Only */
int QJS_GetRefCount(JSValue v);

/*
 * Free memory allocated by QuickJS APIs (eg JS_WriteObject).
 *
 * Bindgen allowlists currently don't expose `js_free` (lowercase), so we wrap it.
 */
void QJS_Free(JSContext *ctx, void *ptr);

/*
 * Stable identity helpers for JSValue that avoid reading uninitialized union/padding bytes
 * (important when JSValue is a struct on 64-bit builds).
 */
int32_t QJS_ValueIdentTag(JSValue v);
uint64_t QJS_ValueIdentPayload(JSValue v);

/*
* create class
*
* @param name: Name of the JavaScript constructor function
* @param constructorCb: constructor callback function
* @param callAsFuncCb: callback function when call object as function
* #param finalizer: finalizer callback to release resource required by constructor
* @return A JSValue representing the constructor function for the class.
*
* in quickjs-ng, class ID is managed at Runtime level
* caller is responsible for avoid duplicated registration
*/
JSValue QJS_CreateClass(JSContext *ctx, const char *class_name, JSCFunction *constructorCb,
                        JSClassCall *callAsFuncCb, JSClassFinalizer *finalizer, JSClassGCMark *gc_mark);

/*
 * create object of class represented by @param constructor
 *
 * @param: privateDate is option opaque to save into object
 */
JSValue QJS_ObjectMake(JSContext *ctx, JSValue constructor, void *privateDate);

/*
 * get private date from object
 */
void *QJS_ObjectGetPrivate(JSValue object);

#ifdef CONFIG_RONG_JIT

/*
 * QuickJS opcode enum mirrored from quickjs-opcode.h so Rust JIT code stays
 * automatically in sync with the vendored quickjs-ng bytecode numbering.
 */
typedef enum QJSJitOp {
#define FMT(f)
#define DEF(id, size, n_pop, n_push, f) QJS_OP_ ## id,
#define def(id, size, n_pop, n_push, f)
#include "quickjs-opcode.h"
#undef def
#undef DEF
#undef FMT
    QJS_OP_COUNT
} QJSJitOp;

typedef int (*QJSJitCallHook)(
    JSContext *caller_ctx,
    void *jit_opaque,
    void *bytecode,
    const JSValue *func_obj,
    const JSValue *this_obj,
    int argc,
    const JSValue *argv,
    int flags,
    JSValue *ret_out
);

typedef void (*QJSJitFreeBytecodeHook)(
    void *jit_opaque,
    void *bytecode,
    void *jit_data
);

typedef void (*QJSJitFreeRuntimeHook)(void *jit_opaque);

typedef struct QJSJitBytecodeInfo {
    const uint8_t *buf;
    uint32_t len;
    uint16_t arg_count;
    uint16_t var_count;
    uint16_t stack_size;
    uint16_t var_ref_count;
    uint16_t closure_var_count;
    uint8_t is_strict_mode;
    uint8_t func_kind;
    int32_t cpool_count;
    const JSValue *cpool;
    JSContext *realm;
} QJSJitBytecodeInfo;

typedef int (*QJSJitNativeFn)(
    JSContext *ctx,
    const JSValue *func_obj,
    const JSValue *this_obj,
    int argc,
    const JSValue *argv,
    JSValue *ret_out
);

void QJS_SetJitHooks(
    JSRuntime *rt,
    void *opaque,
    QJSJitCallHook call_hook,
    QJSJitFreeBytecodeHook free_bc_hook,
    QJSJitFreeRuntimeHook free_rt_hook
);
void *QJS_GetJitOpaque(JSRuntime *rt);

void QJS_JitGetBytecodeInfo(const void *bc, QJSJitBytecodeInfo *out);
void *QJS_JitGetBytecodeData(const void *bc);
void QJS_JitSetBytecodeData(void *bc, void *data);
uint32_t QJS_JitIncCallCount(void *bc);
uint32_t QJS_JitValueSize(void);
int QJS_JitCheckValueLayout(void);
int QJS_JitInvoke(
    JSContext *ctx,
    QJSJitNativeFn fn,
    const void *bc,
    const JSValue *func_obj,
    const JSValue *this_obj,
    int argc,
    const JSValue *argv,
    JSValue *ret_out
);

int QJS_JitPollInterrupts(JSContext *ctx);
int QJS_JitCheckStack(JSContext *ctx, size_t frame_bytes);
void QJS_JitDupValue(JSContext *ctx, const JSValue *v);
void QJS_JitFreeValue(JSContext *ctx, const JSValue *v);
int QJS_JitToBool(JSContext *ctx, const JSValue *v);
int QJS_JitBoxThis(JSContext *ctx, const JSValue *this_obj, JSValue *out);
void QJS_JitThrowUninitialized(JSContext *ctx, uint32_t idx);
int QJS_JitCall(
    JSContext *ctx,
    const JSValue *func_obj,
    const JSValue *this_obj,
    int argc,
    const JSValue *argv,
    JSValue *ret_out
);
int QJS_JitBinarySlow(
    JSContext *ctx,
    int opcode,
    const JSValue *a,
    const JSValue *b,
    JSValue *out
);
int QJS_JitUnarySlow(
    JSContext *ctx,
    int opcode,
    const JSValue *a,
    JSValue *out
);
int QJS_JitPostIncSlow(
    JSContext *ctx,
    int opcode,
    const JSValue *a,
    JSValue *old_out,
    JSValue *new_out
);
int QJS_JitGetVar(JSContext *ctx, uint32_t atom, JSValue *out);
int QJS_JitGetVarUndef(JSContext *ctx, uint32_t atom, JSValue *out);
int QJS_JitPutVar(JSContext *ctx, uint32_t atom, const JSValue *val);
int QJS_JitGetField(JSContext *ctx, const JSValue *obj, uint32_t atom, JSValue *out);
int QJS_JitPutField(JSContext *ctx, const JSValue *obj, uint32_t atom, const JSValue *val);
int QJS_JitGetArrayEl(JSContext *ctx, const JSValue *obj, const JSValue *idx, JSValue *out);
int QJS_JitPutArrayEl(JSContext *ctx, const JSValue *obj, const JSValue *idx, const JSValue *val);
int QJS_JitGetLength(JSContext *ctx, const JSValue *obj, JSValue *out);
int QJS_JitGetClosureVar(JSContext *ctx, const JSValue *func_obj, uint32_t idx, JSValue *out);
int QJS_JitPutClosureVar(JSContext *ctx, const JSValue *func_obj, uint32_t idx, const JSValue *val);

#endif /* CONFIG_RONG_JIT */

#endif
