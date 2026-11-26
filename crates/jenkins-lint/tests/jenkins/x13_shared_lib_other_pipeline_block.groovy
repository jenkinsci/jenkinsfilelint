def call(Map config) {
    pipeline {
        name config.name
        stages {
            build()
        }
    }
}
