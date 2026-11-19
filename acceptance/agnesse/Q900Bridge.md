{
  "type": "test",
  "tags": [],
  "help": null,
  "prune": false
}
# Q900 Bridge CLI acceptance

```agnesse
define COMMAND_PATTERN
target/debug/q900-bridge-slim
```

| script | Q900BridgeFixture |
| check | senderPlanContains; | stereo | channels=2 | true |
| check | senderPlanContains; | 5.1 | channels=6 | true |
| check | senderPlanContains; | 7.1 | channels=8 | true |
| check | senderPlanContains; | stereo | format=s24be | true |
| check | senderPlanContains; | 7.1 | format=s24be | true |
| check | helpContains | stereo | true |
| check | helpContains | 7.1 | true |
