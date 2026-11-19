Feature: PipeWire RTP audio bridge

  Scenario: Mirror system audio to three selected stereo outputs
    Given three selected PipeWire sinks expose stereo FL and FR
    When the user starts mirror with three repeated --output options
    Then it routes system audio through a two-channel 48 kHz virtual sink
    And it creates one independent loopback for each selected output
    And each output can have its own fixed delay
    And Ctrl-C restores the previous default sink and stream routes

  Scenario: Mirror stereo system audio to the Q900 over Bluetooth
    Given two selected PipeWire sinks expose stereo FL and FR
    And one selected sink is the Q900 Bluetooth output
    When the user starts mirror with two --output options
    Then it routes system audio through a two-channel 48 kHz virtual sink
    And it creates an independent stereo loopback for each selected sink
    And it does not require an RTP receiver
    And it reports that a Bluetooth A2DP output may use SBC

  Scenario: Mirror stereo PCM to the Q900 through an optical output sink
    Given two selected PipeWire sinks expose stereo FL and FR
    And one selected sink is an optical S/PDIF output
    When the user starts mirror with two --output options
    Then it routes system audio through a two-channel 48 kHz virtual sink
    And it creates an independent stereo loopback for each selected sink
    And it reports S/PDIF PCM and asks the user to confirm the cable and endpoint input
    And it does not report a Bluetooth codec for the optical path

  Scenario: Send raw stereo PCM over Wi-Fi to a receiver
    Given the receiver output sink exposes stereo FL and FR
    When sender and receiver use layout stereo
    Then the sender advertises a two-channel 24-bit 48 kHz RTP stream
    And the sender keeps the local stereo output active
    And the receiver routes RTP to its selected stereo output sink
    And the RTP path does not use the Q900 Bluetooth codec

  Scenario: Send 5.1 PCM without using Bluetooth
    Given the sender has a non-Bluetooth local audio sink
    When the sender starts with layout 5.1
    Then it creates a six-channel 24-bit 48 kHz virtual sink
    And it advertises a six-channel RTP stream on the Wi-Fi LAN
    And it mirrors audio to the configured local sink

  Scenario: Send 7.1 PCM without downmixing at the receiver
    Given the receiver HDMI sink exposes eight expected channel positions
    When the receiver starts with layout 7.1
    Then it loads the RTP receiver for that HDMI sink
    And it accepts an eight-channel RTP stream

  Scenario Outline: Map Pro Audio AUX channels to HDMI speaker positions
    Given the receiver Pro Audio sink exposes <channels> channels named in order from AUX0
    And its associated HDMI port is reported connected
    When the receiver starts with layout <layout>
    Then it maps RTP channels to the standard HDMI positions <positions>
    And it preserves all <channels> channels without downmixing

    Examples:
      | layout | channels | positions                                                        |
      | 5.1    | 6        | front-left,front-right,front-center,lfe,rear-left,rear-right     |
      | 7.1    | 8        | front-left,front-right,front-center,lfe,rear-left,rear-right,side-left,side-right |

  Scenario: Refuse Pro Audio AUX playback when HDMI is not connected
    Given the receiver Pro Audio sink exposes the requested AUX channels
    And its associated HDMI port is not reported connected
    When the receiver starts with a multichannel layout
    Then it reports that the HDMI route is unavailable
    And it does not load an RTP receiver module

  Scenario: Reject a stereo receiver sink
    Given the receiver HDMI sink exposes only two channels
    When the receiver starts with layout 5.1
    Then it reports that the sink needs at least six channels
    And it does not load an RTP receiver module

  Scenario Outline: Check discrete LPCM channel order on a connected HDMI sink
    Given the HDMI sink exposes <channels> channels in the requested layout
    And the HDMI output is reported connected
    When the user runs lpcm-check with layout <layout>
    Then it plays one low-level 24-bit 48 kHz tone at a time on every channel
    And it prints the channel name and tone frequency in playback order
    And it leaves the system default sink and application routes unchanged

    Examples:
      | layout | channels |
      | 5.1    | 6        |
      | 7.1    | 8        |

  Scenario: Restore the sender's original audio routing
    Given the sender has moved existing system audio streams to its virtual sink
    When the sender stops with Ctrl-C
    Then it restores the prior default sink and stream destinations
    And it unloads the virtual sink and RTP modules

  Scenario: Analyze repeated acoustic synchronization observations
    Given a CSV file with elapsed time and local and Q900T arrival times from one recorder clock
    When the user runs the sync-report command
    Then it reports the initial acoustic skew and drift over time
    And it reports median, 95th percentile, and maximum skew and residual jitter
    And it recommends which output to delay to compensate the initial fixed offset

  Scenario: Apply a measured fixed output offset
    Given an acoustic measurement recommends delaying the local output by 25 milliseconds
    When the sender starts with --local-delay-ms 25
    Then its PipeWire loopback adds a 25 millisecond output delay
    And its RTP branch keeps the original timing
    When a measurement instead recommends delaying the Q900T output by 25 milliseconds
    And the receiver starts with --output-delay-ms 25
    Then its HDMI output loopback adds a 25 millisecond output delay
    And the local sender output keeps the original timing

  Scenario: Calibrate relative output delays from a microphone
    Given a PipeWire microphone is placed at a fixed measurement point
    And two or more selected outputs can play a calibration probe
    When the user starts microphone calibration
    Then it plays one distinct coded probe per selected output and repeats the capture three times
    And it records each probe arrival on one capture clock
    And it estimates relative output delay from the recorded signals
    And it proposes non-negative fixed delays for earlier outputs
    And it changes no delay until the user applies the proposals
    And it does not claim to correct long-term clock drift
