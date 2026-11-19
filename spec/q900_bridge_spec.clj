(ns q900-bridge-spec
  (:require [clojure.java.shell :as shell]
            [speclj.core :refer :all]))

(def bridge-binary
  (or (System/getenv "Q900_BRIDGE_BIN") "target/debug/q900-bridge"))

(defn run-bridge [& args]
  (apply shell/sh bridge-binary args))

(describe "Q900 Bridge CLI acceptance"
  (it "plans uncompressed 24-bit stereo, 5.1, and 7.1 PCM"
    (doseq [[layout channels] [["stereo" 2] ["5.1" 6] ["7.1" 8]]]
      (let [{:keys [exit out err]} (run-bridge "sender"
                                                 "--layout" layout
                                                 "--destination-ip" "239.0.0.56"
                                                 "--local-sink" "alsa_output.acceptance"
                                                 "--dry-run")]
        (should= 0 exit)
        (should-contain (str "channels=" channels) out)
        (should-contain "format=s24be" out)
        (should= "" err))))

  (it "documents 7.1 support in the command help"
    (let [{:keys [exit out]} (run-bridge "--help")]
      (should= 0 exit)
      (should-contain "stereo" out)
      (should-contain "7.1" out)
      (should-contain "--output SINK_NAME[=DELAY_MS]" out)
      (should-contain "Microphone-based automatic calibration is planned" out))))
