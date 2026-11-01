# language: en
Feature: The conversation in each prompt
  The model gets the conversation of each request one time, with no item that the API client did not send.
  A request that continues the conversation of an ACP session sends only its new items. Else acp2api does a rebuild.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"
    And an API client sent a request to "POST /v1/chat/completions" with the system prompt "Be kind." and the user message "Hello"
    And the official runtime replied "Hi." in the ACP session "S1"

  @FR-021 @issue-47
  Scenario: A request that adds one user message continues the ACP session
    When the API client sends the same conversation with the reply "Hi." and the new user message "How are you?"
    Then the official runtime gets the prompt "How are you?" in the ACP session "S1"
    And the official runtime gets no new ACP session

  @FR-021 @issue-47
  Scenario: A request with an edited earlier message does a rebuild
    When the API client sends the conversation with the user message "Hello there", the reply "Hi.", and the new user message "How are you?"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session is:
      """
      The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.

      [user]
      Hello there
      [assistant]
      Hi.
      [user]
      How are you?
      """
    And the new ACP session gets the system prompt addition "Be kind."

  @FR-021 @issue-47
  Scenario: An image of the earlier part of a conversation is at its place in a rebuild
    Given an API client sent a conversation whose user message has the text "See this:" and an image, and the official runtime replied "A cat."
    When the API client sends that conversation with the reply "A dog." in place of "A cat.", and the new user message "Sure?"
    Then the first prompt of the new ACP session has the text "See this:", the image, and the line "[assistant]" in this order

  @FR-021 @issue-47
  Scenario: A new conversation with no earlier part is not a rebuild
    When an API client sends a new conversation with only the user message "Good morning"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session is "Good morning"
    And the log has no rebuild line

  @FR-021 @issue-66
  Scenario Outline: The next request after an event that stops an ACP session does a rebuild
    Given the next prompt turn in the ACP session "S1" ends with <event>
    And 30 s pass after that end
    When the API client sends the next request of that conversation
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session starts with the neutral line

    Examples:
      | event                                   |
      | a cut at a stop sequence                |
      | a disconnect of the API client          |
      | the stderr error line 503 "UNAVAILABLE" |
      | the stop reason "refusal", and "/usage" shows quota for the model |

  @FR-021 @issue-51
  Scenario: The text of a rebuild after a cut holds the reply only up to the cut
    Given the API client sent the user message "Count to 3." with the stop sequence "3"
    And the official runtime replied "1, 2, 3, 4", so acp2api cut the reply to "1, 2, "
    When the API client sends that conversation with the reply "1, 2, " and the new user message "Go on."
    Then the first prompt of the new ACP session holds the reply "1, 2, "
    And that prompt holds no text "3, 4"

  @FR-021 @issue-47
  Scenario: After a restart of acp2api the next request of a conversation does a rebuild
    Given the owner restarted acp2api
    When the API client sends the same conversation with the reply "Hi." and the new user message "How are you?"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session starts with the neutral line

  @FR-021 @issue-47
  Scenario: acp2api uses the content of the conversation, not a session hint
    Given the first request had the header "x-claude-code-session-id: abc"
    When the API client sends the conversation with the edited user message "Hello there" and the header "x-claude-code-session-id: abc"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session starts with the neutral line

  @FR-021 @issue-47
  Scenario: A changed session hint does not stop a continuation
    Given the first request had "prompt_cache_key" set to "key-1"
    When the API client sends the same conversation with the reply "Hi.", the new user message "How are you?", and "prompt_cache_key" set to "key-2"
    Then the official runtime gets the prompt "How are you?" in the ACP session "S1"

  @FR-021 @issue-47
  Scenario: acp2api tells the API client nothing about a rebuild and writes a log line
    Given the official runtime replies "Fine." to the next prompt
    When the API client sends the conversation with the user message "Hello there", the reply "Hi.", and the new user message "How are you?"
    Then the reply text of the response is "Fine."
    And the log has one rebuild line

  @FR-021 @issue-93
  Scenario: An ACP session with no request for 24 hours loses its record
    Given the official runtime kept a session file for the ACP session "S1"
    When 24 hours pass since the last request of the ACP session "S1"
    And the API client sends the same conversation with the reply "Hi." and the new user message "How are you?"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session starts with the neutral line

  @FR-021 @issue-93
  Scenario: A continuation starts another 24 hours of record retention
    Given the official runtime kept a session file for the ACP session "S1"
    When 23 hours pass since the last request of the ACP session "S1"
    Given the official runtime replies "Fine." to the next prompt
    When the API client sends the same conversation with the reply "Hi." and the new user message "How are you?"
    Then the official runtime gets the prompt "How are you?" in the ACP session "S1"
    When 23 hours pass since the last request of the ACP session "S1"
    Given the official runtime replies "Still fine." to the next prompt
    When the API client sends this conversation:
      """
      [
        {"role":"user","content":"Hello"},
        {"role":"assistant","content":"Hi."},
        {"role":"user","content":"How are you?"},
        {"role":"assistant","content":"Fine."},
        {"role":"user","content":"Again"}
      ]
      """
    Then the official runtime gets the prompt "Again" in the ACP session "S1"
    When 24 hours pass since the last request of the ACP session "S1"
    And the API client sends this conversation:
      """
      [
        {"role":"user","content":"Hello"},
        {"role":"assistant","content":"Hi."},
        {"role":"user","content":"How are you?"},
        {"role":"assistant","content":"Fine."},
        {"role":"user","content":"Again"},
        {"role":"assistant","content":"Still fine."},
        {"role":"user","content":"After a day"}
      ]
      """
    Then the official runtime gets a new ACP session

  @FR-021 @issue-93
  Scenario: The next start deletes each session file and each ACP session folder
    Given the official runtime kept a session file for the ACP session "S1"
    When the owner restarts acp2api
    Then the session files and ACP session folders are gone and the sign-in files are unchanged

  @FR-021 @FR-047 @issue-113
  Scenario: A match loads its ACP session after the earlier runtime process stops
    Given an ACP session holds the client tool call "lookup" and its result "sunny"
    And the official runtime kept a session file for that ACP session
    When the runtime process exits, and acp2api did not cause the exit
    And the API client sends the next match of that ACP session
    Then the official runtime gets "session/load" for that ACP session in a new runtime process
    And the official runtime gets only the prompt "How are you?" in that ACP session
    And the API client gets the reply "Fine." after the replay of the completed tool call

  @FR-021 @issue-218
  Scenario Outline: A changed form of the arguments of the last tool call continues the ACP session
    Given an API client uses "<api>"
    And the API client offers the client tools Bash, Edit, Read, and Write
    And the official runtime called the client tool "<tool>" with the arguments <emitted>
    When the API client sends that tool call with the arguments <returned> and the tool result "Done."
    Then the official runtime gets only the tool result "Done." in the same ACP session
    And the count of ACP sessions stays the same

    Examples:
      | api                       | tool  | emitted                                                                   | returned                                                                                   |
      | POST /v1/messages         | Edit  | {"file_path":"a.py","old_string":"    x = 1","new_string":"    x = 4"}    | {"replace_all":false,"file_path":"a.py","old_string":"    x = 1","new_string":"    x = 4"} |
      | POST /v1/messages         | Write | {"file_path":"a.py","content":"def f():\\n    x = 1   \\n\\ty = 2\\t\\n"} | {"file_path":"a.py","content":"def f():\\n    x = 1\\n\\ty = 2\\n"}                        |
      | POST /v1/messages         | Bash  | {"command":"cd /work && make test"}                                       | {"command":"make test"}                                                                    |
      | POST /v1/messages         | Bash  | {"description":"Run the tests","command":"make test"}                     | {"command":"make test","description":"Run the tests"}                                      |
      | POST /v1/messages         | Read  | {"file_path":"a.py"}                                                      | {"file_path":"b.py"}                                                                       |
      | POST /v1/chat/completions | Edit  | {"file_path":"a.py","old_string":"x","new_string":"y"}                    | {"replace_all":false,"file_path":"a.py","old_string":"x","new_string":"y"}                 |
      | POST /v1/chat/completions | Bash  | {"description":"Run the tests","command":"make test"}                     | {"command":"make test","description":"Run the tests"}                                      |
      | POST /v1/responses        | Bash  | {"command":"cd /work && make test"}                                       | {"command":"make test"}                                                                    |
      | POST /v1/responses        | Read  | {"file_path":"a.py"}                                                      | {"file_path":"b.py"}                                                                       |

  @FR-021 @issue-218
  Scenario Outline: Another call ID or another name of the last tool call stops the match
    Given an API client uses "<api>"
    And the API client offers the client tools Bash, Edit, Read, and Write
    And the official runtime called the client tool "Edit" with the arguments {"file_path":"a.py"}
    When the API client sends that tool call with <change> and the tool result "Done."
    Then the request does a rebuild

    Examples:
      | api                       | change                   |
      | POST /v1/messages         | the call ID "call-other" |
      | POST /v1/messages         | the name "Write"         |
      | POST /v1/chat/completions | the call ID "call-other" |
      | POST /v1/chat/completions | the name "Write"         |
      | POST /v1/responses        | the call ID "call-other" |
      | POST /v1/responses        | the name "Write"         |

  @FR-021 @issue-218
  Scenario Outline: After a match, a later change of the arguments of that tool call stops the match
    Given an API client uses "<api>"
    And the API client offers the client tools Bash, Edit, Read, and Write
    And the official runtime called the client tool "Bash" with the arguments <emitted>
    When the API client sends that tool call with the arguments <returned> and the tool result "Done."
    Then the official runtime gets only the tool result "Done." in the same ACP session
    And the count of ACP sessions stays the same
    When the API client sends the next user message "Next"
    Then the official runtime gets only the prompt "Next" in the same ACP session
    And the count of ACP sessions stays the same
    When the API client sends the same conversation with that tool call changed to the arguments <later> and the new user message "Again"
    Then the request does a rebuild

    Examples:
      | api                       | emitted                                               | returned                                              | later                                                 |
      | POST /v1/messages         | {"command":"cd /work && make"}                        | {"command":"make"}                                    | {"command":"cd /work && make"}                        |
      | POST /v1/messages         | {"description":"Run the tests","command":"make test"} | {"command":"make test","description":"Run the tests"} | {"description":"Run the tests","command":"make test"} |
      | POST /v1/messages         | {"command":"cd /work && echo  a"}                     | {"command":"echo  a"}                                 | {"command":"echo a"}                                  |
      | POST /v1/chat/completions | {"command":"cd /work && make"}                        | {"command":"make"}                                    | {"command":"cd /work && make"}                        |
      | POST /v1/responses        | {"command":"cd /work && make"}                        | {"command":"make"}                                    | {"command":"cd /work && make"}                        |

  @FR-021 @issue-218
  Scenario Outline: After a match, a later change of the call ID of that tool call stops the match
    Given an API client uses "<api>"
    And the API client offers the client tools Bash, Edit, Read, and Write
    And the official runtime called the client tool "Bash" with the arguments {"command":"cd /work && make"}
    When the API client sends that tool call with the arguments {"command":"make"} and the tool result "Done."
    Then the official runtime gets only the tool result "Done." in the same ACP session
    And the count of ACP sessions stays the same
    When the API client sends the same conversation with the call ID "call-other" for that tool call and the new user message "Again"
    Then the request does a rebuild

    Examples:
      | api                       |
      | POST /v1/messages         |
      | POST /v1/chat/completions |

  @FR-021 @issue-218
  Scenario Outline: A change of white space in an earlier user message stops the match
    Given an API client uses "<api>"
    And the API client sent the user message <sent>, and the official runtime replied "Fine."
    When the API client sends the same conversation with <sent> changed to <changed> and the new user message "Next"
    Then the request does a rebuild

    Examples:
      | api                       | sent                 | changed            |
      | POST /v1/messages         | "if a:\\n    return" | "if a:\\n  return" |
      | POST /v1/chat/completions | "if a:\\n    return" | "if a:\\n  return" |
      | POST /v1/messages         | "name =  value"      | "name = value"     |
      | POST /v1/responses        | "name =  value"      | "name = value"     |
      | POST /v1/messages         | "one   \\ntwo"       | "one\\ntwo"        |
      | POST /v1/chat/completions | "one\\r\\ntwo"       | "one\\ntwo"        |

  @FR-021 @issue-218
  Scenario Outline: A change of white space in an earlier tool result stops the match
    Given an API client uses "<api>"
    And the API client offers the client tools Bash, Edit, Read, and Write
    And the official runtime called the client tool "Read" with the arguments {"file_path":"a.py"}
    And the API client sent the tool result <sent> for that tool call, and the official runtime replied "Fine."
    When the API client sends the same conversation with <sent> changed to <changed> and the new user message "Next"
    Then the request does a rebuild

    Examples:
      | api                       | sent                 | changed            |
      | POST /v1/messages         | "if a:\\n    return" | "if a:\\n  return" |
      | POST /v1/chat/completions | "if a:\\n    return" | "if a:\\n  return" |
      | POST /v1/responses        | "if a:\\n    return" | "if a:\\n  return" |
      | POST /v1/messages         | "one   \\ntwo"       | "one\\ntwo"        |

  @FR-021 @issue-218
  Scenario Outline: A change of white space in an earlier assistant text that is not the last reply stops the match
    Given an API client uses "<api>"
    And the API client sent the user message "One", and the official runtime replied <sent>
    And the API client sent the user message "Two", and the official runtime replied "Fine."
    When the API client sends the same conversation with <sent> changed to <changed> and the new user message "Next"
    Then the request does a rebuild

    Examples:
      | api                       | sent                 | changed            |
      | POST /v1/messages         | "if a:\\n    return" | "if a:\\n  return" |
      | POST /v1/chat/completions | "if a:\\n    return" | "if a:\\n  return" |
      | POST /v1/messages         | "one   \\ntwo"       | "one\\ntwo"        |

  @FR-021 @issue-218
  Scenario Outline: A change of white space in the system prompt stops the match
    Given an API client uses "<api>"
    And the API client sets the system prompt "Rules:\\n    one"
    And the API client sent the user message "One", and the official runtime replied "Fine."
    When the API client sends the same conversation with "Rules:\\n    one" changed to "Rules:\\n  one" and the new user message "Next"
    Then the request does a rebuild

    Examples:
      | api                       |
      | POST /v1/messages         |
      | POST /v1/chat/completions |

  @FR-021 @issue-218
  Scenario Outline: A change of white space in a description of a client tool stops the match
    Given an API client uses "<api>"
    And the API client offers the client tool "Read" with the description "Read  a file."
    And the API client sent the user message "One", and the official runtime replied "Fine."
    When the API client sends the same conversation with "Read  a file." changed to "Read a file." and the new user message "Next"
    Then the request does a rebuild

    Examples:
      | api                       |
      | POST /v1/messages         |
      | POST /v1/chat/completions |

  @FR-021 @issue-218
  Scenario Outline: The last reply text in a clean form continues the ACP session
    Given an API client uses "<api>"
    And the API client sent the user message "One", and the official runtime replied <reply>
    When the API client sends the same conversation with <reply> changed to <clean> and the new user message "Next"
    Then the official runtime gets only the prompt "Next" in the same ACP session
    And the count of ACP sessions stays the same

    Examples:
      | api                       | reply                                 | clean                |
      | POST /v1/messages         | "one\\r\\ntwo"                        | "one\\ntwo"          |
      | POST /v1/messages         | "lone\\rcarriage"                     | "lonecarriage"       |
      | POST /v1/messages         | "one   \\ntwo\\t\\nthree"             | "one\\ntwo\\nthree"  |
      | POST /v1/messages         | "  Text"                              | "Text"               |
      | POST /v1/messages         | "Text\\n"                             | "Text"               |
      | POST /v1/messages         | "\\n  if a:\\r\\n    return   \\n\\n" | "if a:\\n    return" |
      | POST /v1/chat/completions | "lone\\rcarriage"                     | "lonecarriage"       |
      | POST /v1/chat/completions | "one   \\ntwo\\t\\nthree"             | "one\\ntwo\\nthree"  |
      | POST /v1/chat/completions | "\\n\\nText\\n\\n"                    | "Text"               |
      | POST /v1/responses        | "one\\r\\ntwo"                        | "one\\ntwo"          |
      | POST /v1/responses        | "Text   "                             | "Text"               |

  @FR-021 @issue-218
  Scenario Outline: The last reply text in a clean form continues the ACP session before the last tool call
    Given an API client uses "<api>"
    And the API client offers the client tools Bash, Edit, Read, and Write
    And the official runtime replied "Let me run it.\n" and called the client tool "Bash" with the arguments {"command":"cd /work && make"}
    When the API client sends that reply text as "Let me run it.", that tool call with the arguments {"command":"make"}, and the tool result "Done."
    Then the official runtime gets only the tool result "Done." in the same ACP session
    And the count of ACP sessions stays the same

    Examples:
      | api                       |
      | POST /v1/messages         |
      | POST /v1/chat/completions |
      | POST /v1/responses        |

  @FR-021 @issue-218
  Scenario Outline: Changed indentation or changed spaces in the last reply text stops the match
    Given an API client uses "<api>"
    And the API client sent the user message "One", and the official runtime replied <reply>
    When the API client sends the same conversation with <reply> changed to <changed> and the new user message "Next"
    Then the request does a rebuild

    Examples:
      | api                       | reply                | changed              |
      | POST /v1/messages         | "if a:\\n    return" | "if a:\\n  return"   |
      | POST /v1/messages         | "if a:\\n\\treturn"  | "if a:\\n    return" |
      | POST /v1/messages         | "name =  value"      | "name = value"       |
      | POST /v1/chat/completions | "if a:\\n    return" | "if a:\\n  return"   |
      | POST /v1/chat/completions | "name =  value"      | "name = value"       |
      | POST /v1/responses        | "if a:\\n    return" | "if a:\\n  return"   |

  @FR-021 @issue-218
  Scenario Outline: Changed indentation in the text before the last tool call stops the match
    Given an API client uses "<api>"
    And the API client offers the client tools Bash, Edit, Read, and Write
    And the official runtime replied "Steps:\n    make" and called the client tool "Bash" with the arguments {"command":"make"}
    When the API client sends that reply text as "Steps:\n  make", that tool call with the arguments {"command":"make"}, and the tool result "Done."
    Then the request does a rebuild

    Examples:
      | api                       |
      | POST /v1/messages         |
      | POST /v1/chat/completions |

  @FR-021 @issue-218
  Scenario Outline: After a match, a later change of the same text stops the match
    Given an API client uses "<api>"
    And the API client sent the user message "One", and the official runtime replied <reply>
    When the API client sends the same conversation with <reply> changed to <clean> and the new user message "Next"
    Then the official runtime gets only the prompt "Next" in the same ACP session
    And the count of ACP sessions stays the same
    When the API client sends the same conversation with <clean> changed to <later> and the new user message "Again"
    Then the request does a rebuild

    Examples:
      | api                       | reply         | clean       | later         |
      | POST /v1/messages         | "Text\\n"     | "Text"      | "Text\\n"     |
      | POST /v1/messages         | "one  \\ntwo" | "one\\ntwo" | "one  \\ntwo" |
      | POST /v1/chat/completions | "one  \\ntwo" | "one\\ntwo" | "one \\ntwo"  |
      | POST /v1/responses        | "Text\\n"     | "Text"      | " Text"       |
