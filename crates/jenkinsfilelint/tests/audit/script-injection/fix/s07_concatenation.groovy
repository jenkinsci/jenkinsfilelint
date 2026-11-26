pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh 'echo ' + env.CHANGE_TITLE
            }
        }
    }
}
