def call(Map config) {
    timestamps {
        pipeline {
            agent any
            stages {
                stage('S') {
                    steps { echo 'x' }
                }
            }
        }
    }
}
