Feature: PipeWire RTP audio bridge

  Scenario: Mirror stereo system audio to the Q900 over Bluetooth
    Given the local output and Q900 Bluetooth sink expose stereo FL and FR
    When the user starts mirror with both sink names
    Then it routes system audio through a two-channel 48 kHz virtual sink
    And it creates independent stereo loopbacks to the local output and Q900
    And it does not require an RTP receiver
    And it reports that the active Q900 Bluetooth profile may use SBC

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
