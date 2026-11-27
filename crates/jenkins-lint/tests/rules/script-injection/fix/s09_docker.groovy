pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    docker.image("maven:3").withRun("-e T=${CHANGE_TITLE}") { c -> }
                }
            }
        }
    }
}
