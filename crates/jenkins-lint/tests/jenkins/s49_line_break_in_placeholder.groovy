pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                sh "echo ${
                    env.BUILD_NUMBER }"
            }
        }
    }
}
