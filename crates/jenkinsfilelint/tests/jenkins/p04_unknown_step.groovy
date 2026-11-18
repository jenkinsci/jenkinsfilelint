pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                ech 'hi'
            }
        }
    }
}
