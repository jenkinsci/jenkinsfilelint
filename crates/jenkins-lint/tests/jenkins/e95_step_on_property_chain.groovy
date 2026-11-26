pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                a.b.c 'x'
            }
        }
    }
}
