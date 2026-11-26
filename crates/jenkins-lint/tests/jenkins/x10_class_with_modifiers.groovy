package org.example

public class Helper implements Serializable {
    def steps
    Helper(steps) { this.steps = steps }
}

abstract class Base implements Serializable {
    abstract def run()
}

final class Constants {
    static final String NAME = 'x'
}
