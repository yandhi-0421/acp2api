# language: en
Feature: Places and queue
  The concurrency limit gives the number of places. A request holds a place from its first call to the official runtime to the end of its prompt turn.
  The other requests wait in the queue in the order of arrival. The queue holds no more than 32 requests.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-049
  Scenario: With the concurrency limit 2, the third request waits until a place is free
    Given the concurrency limit is 2
    When API clients send the requests "A", "B", and "C" of three conversations
    Then the official runtime gets no call for "C" while the prompt turns of "A" and "B" run
    When the prompt turn of "A" ends
    Then the official runtime gets the first call for "C"

  @FR-049
  Scenario: A change of the concurrency limit applies at once
    Given the concurrency limit is 2
    And the prompt turns of "A" and "B" run, and "C" waits in the queue
    When the owner sets the concurrency limit 3 in the management panel
    Then the prompt turn of "C" starts at once

  @FR-049
  Scenario: In the state sign-in required, the sign-in check of a request holds a place
    Given the concurrency limit is 1
    And acp2api is in the state "sign-in required"
    And the sign-in check of the request "A" runs
    When the request "B" comes
    Then the sign-in check of "B" starts only after the end of the sign-in check of "A"

  @FR-049
  Scenario: Each quota query holds a place
    Given the concurrency limit is 1
    And a "/usage" for a refresh by the owner runs
    When the request "A" comes
    Then the official runtime gets no call for "A" until the end of the "/usage"

  @FR-049
  Scenario: The start of a runtime process holds no place
    Given the concurrency limit is 2
    When acp2api starts a runtime process
    Then the count of the places in use is 0 during the start

  @FR-050
  Scenario: Requests start in the order of arrival
    Given the concurrency limit is 1
    And the prompt turn of the request "A" runs
    And the request "B" comes, and then the request "C" comes
    When the prompt turn of "A" ends
    Then the prompt turn of "B" starts before the prompt turn of "C"

  @FR-050
  Scenario: A request waits with no time limit while its API client stays connected
    Given the concurrency limit is 1
    And the prompt turn of the request "A" runs for 60 minutes
    And the request "B" waits in the queue, and its API client stays connected
    When the prompt turn of "A" ends
    Then the prompt turn of "B" starts

  @FR-050
  Scenario Outline: The queue holds no more than 32 requests
    Given each place holds a prompt turn
    And <count> requests wait in the queue
    When an API client sends a valid request to "POST /v1/messages"
    Then <result>

    Examples:
      | count | result                                                                                                              |
      | 31    | the request waits in the queue                                                                                      |
      | 32    | acp2api refuses the request at once with HTTP 529, the header "retry-after: 10", and the message "acp2api: the queue is full (32 requests). Send the request again later." |

  @FR-051
  Scenario Outline: A prompt turn with no update for 5 minutes ends with M-19
    Given the next prompt turn sends updates with a gap of <gap>
    When an API client sends a valid request to "POST /v1/messages" with "stream" set to false
    Then <result>

    Examples:
      | gap       | result                                                                                                                                       |
      | 4 minutes | the prompt turn continues to its end, and the response has the full reply                                                                    |
      | 5 minutes | the official runtime gets a cancel of the prompt turn, and acp2api answers with HTTP 503 and the message "acp2api: the official runtime sent no update for 5 minutes, so acp2api cancelled the prompt turn." |

  @FR-037
  Scenario Outline: The start of a state ends the requests in the queue with the error of that state
    Given each place holds a prompt turn
    And two requests with no stream wait in the queue
    When <event> starts the state "<state>"
    Then each of the two requests gets HTTP <status> with the message of the state "<state>"

    Examples:
      | event                                             | state               | status |
      | the stderr error line 429 "RESOURCE_EXHAUSTED"    | quota wait          | 429    |
      | the stderr error line 401                         | sign-in required    | 400    |
      | the third failure of the runtime process in 10 minutes | runtime unavailable | 400 |

  @FR-037
  Scenario: The prompt turns that run continue to their end at the start of a quota wait
    Given the concurrency limit is 2
    And the prompt turns of the requests "A" and "B" run
    And two requests wait in the queue
    When the prompt turn of "A" ends with the stderr error line 429 "RESOURCE_EXHAUSTED"
    Then the two requests in the queue get HTTP 429
    And the prompt turn of "B" continues to its end, and "B" gets its full reply
