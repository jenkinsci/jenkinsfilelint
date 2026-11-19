pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                sh "printenv | grep -vi 'token\|password'"
            }
        }
    }
}
