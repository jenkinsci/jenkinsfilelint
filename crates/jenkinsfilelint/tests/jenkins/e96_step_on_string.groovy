pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                'a'.foo 'x'
            }
        }
    }
}
