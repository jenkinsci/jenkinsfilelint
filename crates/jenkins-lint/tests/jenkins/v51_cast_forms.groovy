pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    String[] parts = (String[]) items.toArray()
                    def names = (List<String>) data.names
                    def rows = (java.util.List) data.rows
                    def text = (String) data.text
                    def count = (int) data.count
                    def first = (data.rows)[0]
                    def made = (data.factory)('x')
                    def same = (count)
                    def pick = (count) ? text : names
                }
            }
        }
    }
}
