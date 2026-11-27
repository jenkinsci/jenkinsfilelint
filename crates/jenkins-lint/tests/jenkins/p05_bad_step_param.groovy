pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh foo: 'ls'
            }
        }
    }
}
