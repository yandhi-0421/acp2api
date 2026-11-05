"""Exactly two official SDK Chat Completions requests to the local recorder."""
import json
import sys
import openai

port = int(sys.argv[1])
with openai.OpenAI(base_url=f"http://127.0.0.1:{port}/v1", api_key="made-up-sc239-key",
                   max_retries=0, timeout=20, http_client=openai.DefaultHttpxClient(trust_env=False)) as api:
    common = dict(model="gemini-3.8-flash", messages=[{"role": "user", "content": "SC239-SDK"}])
    plain = api.chat.completions.create(**common)
    assert plain.choices[0].message.content == "SC239-OK"
    assert plain.choices[0].finish_reason == "stop"
    chunks = list(api.chat.completions.create(**common, stream=True,
                                            stream_options={"include_usage": True}))
    assert "".join(chunk.choices[0].delta.content or "" for chunk in chunks if chunk.choices) == "SC239-OK"
    assert any(chunk.choices and chunk.choices[0].finish_reason == "stop" for chunk in chunks)
    assert any(chunk.usage is not None for chunk in chunks)
    print(json.dumps({"openai": openai.__version__, "requests": 2, "nonstream": "PASS", "stream": "PASS",
                      "plain": plain.model_dump(), "chunks": [chunk.model_dump() for chunk in chunks]}))
