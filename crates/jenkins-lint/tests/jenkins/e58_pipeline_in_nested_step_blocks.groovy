node {
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
