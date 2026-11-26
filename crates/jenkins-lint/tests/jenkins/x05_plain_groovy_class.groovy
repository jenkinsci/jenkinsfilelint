package org.example

class Helper implements Serializable {
    def steps
    Helper(steps) { this.steps = steps }
    def run(String cmd) { steps.sh(cmd) }
}
