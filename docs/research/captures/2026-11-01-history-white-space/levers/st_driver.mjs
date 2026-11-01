// Drive a real SillyTavern page in headless Chrome over CDP.
// Limits: the deadline in seconds, and the turn limit of run.py.
// Arguments: a JSON file with cdpPort, appPort, fakePort, prompts, ends, results,
// deadlineSeconds, turnSeconds, stream.
import { readFileSync, writeFileSync } from 'node:fs';

const config = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const deadline = Date.now() + config.deadlineSeconds * 1000;
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const log = [];
const note = (text) => { if (log.length < 400) log.push(String(text).slice(0, 400)); };

async function until(check, label, seconds) {
    const limit = Math.min(deadline, Date.now() + seconds * 1000);
    while (Date.now() < limit) {
        const value = await check();
        if (value) return value;
        await wait(150);
    }
    throw new Error(`timeout: ${label}`);
}

const targets = await until(async () => {
    try {
        const reply = await fetch(`http://127.0.0.1:${config.cdpPort}/json`);
        const list = await reply.json();
        return list.find((entry) => entry.type === 'page');
    } catch { return null; }
}, 'chrome', 30);

const socket = new WebSocket(targets.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
let nextId = 1;
const pending = new Map();
socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
        pending.get(message.id)(message);
        pending.delete(message.id);
    } else if (message.method === 'Runtime.exceptionThrown') {
        note(`exception: ${message.params.exceptionDetails.text}`);
    }
};
function send(method, params = {}) {
    const id = nextId++;
    return new Promise((resolve) => {
        pending.set(id, resolve);
        socket.send(JSON.stringify({ id, method, params }));
    });
}
async function run(expression) {
    const reply = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (reply.result?.exceptionDetails) {
        throw new Error(JSON.stringify(reply.result.exceptionDetails).slice(0, 500));
    }
    return reply.result?.result?.value;
}
async function fakeState() {
    const reply = await fetch(`http://127.0.0.1:${config.fakePort}/__state`);
    return reply.json();
}

async function dump(reason) {
    const page = await run(`JSON.stringify({
        title: document.title,
        popups: [...document.querySelectorAll('.popup, #dialogue_popup')].filter((e) => e.offsetParent).map((e) => e.innerText.slice(0, 200)),
        status: window.SillyTavern ? SillyTavern.getContext().onlineStatus : null,
        chars: window.SillyTavern ? SillyTavern.getContext().characters.length : null,
        main: $('#main_api').val(), source: $('#chat_completion_source').val(), url: $('#custom_api_url_text').val(),
    })`).catch((error) => String(error));
    process.stderr.write(`${reason}\n${page}\n${log.join('\n')}\n`);
}
process.on('unhandledRejection', async (error) => { await dump(String(error)); process.exit(1); });
await send('Runtime.enable');
await send('Page.enable');
await send('Page.navigate', { url: `http://127.0.0.1:${config.appPort}/` });
try {
await until(() => run(`Boolean(window.SillyTavern?.getContext && document.querySelector('#send_textarea') && window.jQuery)`), 'page', 180);
await until(() => run(`SillyTavern.getContext().characters.length > 0`), 'characters', 60);
await run(`document.querySelectorAll('.popup-button-ok, #dialogue_popup_ok').forEach((b) => b.click()); true`);

await run(`(async () => {
    const results = ${JSON.stringify(config.results)};
    const c = SillyTavern.getContext();
    $('#main_api').val('openai').trigger('change');
    $('#chat_completion_source').val('custom').trigger('change');
    $('#custom_api_url_text').val('http://127.0.0.1:${config.fakePort}/v1').trigger('input');
    $('#custom_model_id').val('probe-model').trigger('input');
    $('#stream_toggle').prop('checked', ${config.stream}).trigger('input');
    $('#openai_function_calling').prop('checked', true).trigger('input');
    c.registerFunctionTool({
        name: 'probe_echo',
        displayName: 'probe_echo',
        description: 'Return a  fixed   result.',
        parameters: {
            $schema: 'http://json-schema.org/draft-04/schema#',
            type: 'object',
            properties: {
                result_case: { type: 'string' }, text: { type: 'string' }, note: { type: 'string' },
                n: { type: 'number' }, m: { type: 'number' }, big: { type: 'number' },
                obj: { type: 'object' }, list: { type: 'array', items: { type: 'string' } },
            },
            required: ['result_case'],
        },
        action: async (args) => {
            const value = results[args.result_case];
            return Array.isArray(value) ? value.join('') : value;
        },
        formatMessage: () => '',
    });
    $('#api_button_openai').trigger('click');
    return true;
})()`);
await until(() => run(`SillyTavern.getContext().onlineStatus && SillyTavern.getContext().onlineStatus !== 'no_connection'`), 'connection', 30);
await run(`SillyTavern.getContext().selectCharacterById(0)`);
await until(() => run(`SillyTavern.getContext().chat.length > 0`), 'chat', 30);
} catch (error) {
    await dump(String(error));
    process.exit(1);
}

let done = 0;
try {
    for (let index = 0; index < config.prompts.length; index++) {
        await until(() => run(`$('#send_but').is(':visible') && !$('#send_but').hasClass('displayNone')`), 'send button', config.turnSeconds);
        await run(`$('#send_textarea').val(${JSON.stringify(config.prompts[index])}).trigger('input'); $('#send_but').trigger('click'); true`);
        await until(async () => {
            const state = await fakeState();
            if (state.stopped) throw new Error(`fake stopped: ${state.stopped}`);
            return state.steps_done >= config.ends[index] && state.active === 0;
        }, `reply ${index + 1}`, config.turnSeconds);
        await until(() => run(`!$('#mes_stop').is(':visible')`), 'idle', config.turnSeconds);
        await wait(300);
        done++;
    }
} catch (error) {
    note(error.message);
}
const final = await run(`JSON.stringify(SillyTavern.getContext().chat.map((m) => ({ user: m.is_user, tool: Boolean(m.extra?.tool_invocations), mes: String(m.mes).slice(0, 80) })))`);
writeFileSync(config.outFile, JSON.stringify({ done, log, chat: JSON.parse(final) }, null, 1));
process.stdout.write(`driver prompts ${done}\n`);
socket.close();
process.exit(0);
